use super::{Check, CheckResult, CheckSess, QueuedModule};
use crate::{
    ast::{
        self,
        pattern::{HybridPattern, NamePattern, Pattern, StructUnpackPattern, UnpackPatternKind, Wildcard},
    },
    error::diagnostic::{Diagnostic, Label},
    hir,
    infer::substitute::substitute_node,
    span::Span,
    types::{Type, TypeId},
    workspace::{library::LIB_NAME_STD, BindingId, BindingInfoFlags, BindingInfoKind, LibraryId, ModuleId},
};
use std::collections::HashSet;
use ustr::{ustr, Ustr, UstrMap};

pub(super) trait CheckTopLevel
where
    Self: Sized,
{
    fn check_top_level(&self, sess: &mut CheckSess) -> CheckResult<UstrMap<BindingId>>;
}

impl CheckTopLevel for ast::Binding {
    fn check_top_level(&self, sess: &mut CheckSess) -> CheckResult<UstrMap<BindingId>> {
        let node = sess.with_env(self.module_id, |sess, mut env| self.check(sess, &mut env, None))?;

        if let Err(mut diagnostics) = substitute_node(&node, &mut sess.tcx) {
            let last = diagnostics.pop().unwrap();
            sess.workspace.diagnostics.extend(diagnostics);
            return Err(last);
        }

        fn collect_bound_names(node: hir::Node, bound_names: &mut UstrMap<BindingId>, sess: &mut CheckSess) {
            match node {
                hir::Node::Binding(binding) => {
                    let (name, id) = (binding.name, binding.id);
                    sess.cache.bindings.insert(id, binding);
                    bound_names.insert(name, id);
                }
                hir::Node::Sequence(sequence) => {
                    sequence.statements.into_iter().for_each(|statement| {
                        collect_bound_names(statement, bound_names, sess);
                    });
                }
                _ => unreachable!("{:#?}", node),
            }
        }

        let mut bound_names = UstrMap::<BindingId>::default();
        collect_bound_names(node, &mut bound_names, sess);

        Ok(bound_names)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CallerInfo {
    pub module_id: ModuleId,
    pub span: Span,
}

impl<'s> CheckSess<'s> {
    pub fn check_top_level_binding(
        &mut self,
        caller_info: CallerInfo,
        module_id: ModuleId,
        name: Ustr,
    ) -> CheckResult<BindingId> {
        // In general, top level names are search in this order:
        // 1. Current module
        // 2. Extern library
        // 3. Std prelude
        // 4. Built-in names
        if let Some(id) = self.get_global_binding_id(module_id, name) {
            self.workspace.add_binding_info_use(id, caller_info.span);
            self.validate_item_visibility(id, caller_info)?;

            Ok(id)
        } else {
            let module = self
                .modules
                .iter()
                .find(|m| m.id == module_id)
                .unwrap_or_else(|| panic!("{:?}", module_id));

            match module.find_binding(name) {
                Some((index, binding)) => {
                    if !self.encountered_items.insert((module_id, index)) {
                        return Err(Diagnostic::error()
                            .with_message(format!(
                                "cycle detected while checking `{}` in module `{}`",
                                name, module.info.name
                            ))
                            .with_label(Label::primary(caller_info.span, format!("`{}` refers to itself", name)))
                            .with_label(Label::secondary(
                                binding.pattern_span(),
                                format!("`{}` is defined here", name),
                            )));
                    }

                    self.queued_modules
                        .get_mut(&module.id)
                        .unwrap()
                        .queued_bindings
                        .insert(index);

                    let bound_names = binding.check_top_level(self)?;
                    let desired_id = *bound_names.get(&name).unwrap();

                    self.workspace.add_binding_info_use(desired_id, caller_info.span);
                    self.validate_item_visibility(desired_id, caller_info)?;

                    self.encountered_items.remove(&(module_id, index));

                    Ok(desired_id)
                }
                _ => match self.builtin_types.get(&name).copied() {
                    Some(builtin_id) => {
                        self.workspace.add_binding_info_use(builtin_id, caller_info.span);
                        Ok(builtin_id)
                    }
                    None => Err(self.name_not_found_error(module_id, name, caller_info)),
                },
            }
        }
    }

    pub(super) fn name_not_found_error(
        &mut self,
        module_id: ModuleId,
        name: Ustr,
        caller_info: CallerInfo,
    ) -> Diagnostic {
        let module_info = self.workspace.module_infos.get(module_id).unwrap();

        let message = if module_info.name.is_empty() {
            format!("cannot find value `{}` in this scope", name)
        } else {
            format!("cannot find value `{}` in module `{}`", name, module_info.name)
        };

        let label_message = if module_info.name.is_empty() {
            "not found in this scope".to_string()
        } else {
            format!("not found in `{}`", module_info.name)
        };

        Diagnostic::error()
            .with_message(message)
            .with_label(Label::primary(caller_info.span, label_message))
    }

    pub fn validate_item_visibility(&self, id: BindingId, caller_info: CallerInfo) -> CheckResult<()> {
        let binding_info = self.workspace.binding_infos.get(id).unwrap();

        if binding_info.visibility == ast::Visibility::Private && binding_info.module_id != caller_info.module_id {
            Err(Diagnostic::error()
                .with_message(format!("associated symbol `{}` is private", binding_info.name))
                .with_label(Label::primary(caller_info.span, "accessed here"))
                .with_label(Label::secondary(binding_info.span, "defined here")))
        } else {
            Ok(())
        }
    }

    pub fn check_module_by_id(&mut self, id: ModuleId) -> CheckResult<TypeId> {
        let module = self
            .modules
            .iter()
            .find(|m| m.id == id)
            .unwrap_or_else(|| panic!("couldn't find {:?}", id));

        self.check_module(module)
    }

    pub fn check_module(&mut self, module: &ast::Module) -> CheckResult<TypeId> {
        if let Some(ty) = self.get_completed_module_type(module.id) {
            Ok(ty)
        } else {
            let module_type = match self.queued_modules.get(&module.id) {
                Some(queued) => queued.module_type,
                None => {
                    let span = Span::initial(module.file_id);
                    let module_type = self.tcx.bound(Type::Module(module.id), span);

                    // Add the module to the queued modules map
                    self.queued_modules.insert(
                        module.id,
                        QueuedModule {
                            module_type,
                            all_complete: false,
                            queued_bindings: HashSet::new(),
                        },
                    );

                    // Auto import std
                    // TODO: Because of circular import conflicts, we *don't* import the
                    // TODO: prelude automatically for std files. This should be fixed after we create
                    // TODO: a proper dependency graph of all entities/bindings.
                    if !module
                        .info
                        .file_path
                        .starts_with(self.workspace.std_library().root_dir().to_str().unwrap())
                    {
                        self.with_env(module.id, |sess, mut env| {
                            let auto_import_std_pattern = Pattern::Hybrid(HybridPattern {
                                name_pattern: NamePattern {
                                    id: BindingId::unknown(),
                                    name: ustr(LIB_NAME_STD),
                                    span,
                                    is_mutable: false,
                                    ignore: false,
                                },
                                unpack_pattern: UnpackPatternKind::Struct(StructUnpackPattern {
                                    sub_patterns: vec![],
                                    span,
                                    wildcard: Some(Wildcard { span }),
                                }),
                                span,
                            });

                            let std_module_id = sess.workspace.std_library().root_module_id;
                            let std_module_type = sess.check_module_by_id(std_module_id)?;

                            sess.bind_pattern(
                                &mut env,
                                &auto_import_std_pattern,
                                ast::Visibility::Private,
                                std_module_type,
                                None,
                                BindingInfoKind::Orphan,
                                span,
                                BindingInfoFlags::SHADOWABLE,
                            )
                        })?;
                    }

                    module_type
                }
            };

            for (index, binding) in module.bindings.iter().enumerate() {
                if self
                    .queued_modules
                    .get_mut(&module.id)
                    .unwrap()
                    .queued_bindings
                    .insert(index)
                {
                    binding.check_top_level(self)?;
                }
            }

            self.queued_modules.get_mut(&module.id).unwrap().all_complete = true;

            for r#static in module.static_blocks.iter() {
                let node = self.with_env(module.id, |sess, mut env| r#static.check(sess, &mut env, None))?;

                if !self.workspace.build_options.check_mode {
                    self.eval(&node, module.id, r#static.span)?;
                }
            }

            Ok(module_type)
        }
    }

    fn get_completed_module_type(&self, id: ModuleId) -> Option<TypeId> {
        match self.queued_modules.get(&id) {
            Some(QueuedModule {
                module_type,
                all_complete: true,
                ..
            }) => Some(*module_type),
            _ => None,
        }
    }

    pub fn check_library(&mut self, library_id: LibraryId) -> CheckResult<()> {
        self.modules
            .iter()
            .filter(|module| module.info.library_id == library_id)
            .try_for_each(|module| self.check_module(module).map(|_| ()))
    }
}
