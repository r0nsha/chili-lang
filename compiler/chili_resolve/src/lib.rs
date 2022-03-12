mod builtin;
mod declare;
mod import;
mod mark_codegen;
mod resolve;
mod resolver;
mod scope;

use chili_ast::{ast::Ast, workspace::Workspace};
use chili_error::DiagnosticResult;
use codespan_reporting::diagnostic::Diagnostic;
use declare::Declare;
use import::{collect_module_exports, expand_and_replace_glob_imports};
use mark_codegen::mark_bindings_for_codegen;
use resolve::Resolve;
use resolver::Resolver;
use scope::Scope;

pub fn resolve<'w>(workspace: &mut Workspace, asts: &mut Vec<Ast>) -> DiagnosticResult<()> {
    let mut resolver = Resolver::new();

    resolver.add_builtin_types(workspace);
    resolver.exports = collect_module_exports(&asts);

    // Add all module_infos to the workspace
    for ast in asts.iter_mut() {
        ast.module_idx = workspace.add_module_info(ast.module_info);
        resolver
            .global_scopes
            .insert(ast.module_idx, Scope::new(ast.module_info.name));
    }

    // Assign module ids to all imports
    for ast in asts.iter_mut() {
        for import in ast.imports.iter_mut() {
            import.module_idx = workspace.find_module_info(import.module_info).unwrap();
        }
    }

    // Declare all global symbols
    for ast in asts.iter_mut() {
        resolver.module_idx = ast.module_idx;
        resolver.module_info = ast.module_info;
        expand_and_replace_glob_imports(&mut ast.imports, &resolver.exports);
        ast.declare(&mut resolver, workspace)?;
    }

    // Resolve all bindings, scopes, uses, etc...
    for ast in asts.iter_mut() {
        resolver.module_idx = ast.module_idx;
        resolver.module_info = ast.module_info;
        ast.resolve(&mut resolver, workspace)?;
    }

    // Check that an entry point function exists
    if workspace.entry_point_function_idx.is_some() {
        // Follow the main path, marking all bindings that need codegen
        mark_bindings_for_codegen(workspace, asts);
    } else {
        return Err(Diagnostic::error()
            .with_message("entry point function `main` is not defined")
            .with_notes(vec![
                "define function `let main = fn() {}` in your entry file".to_string(),
            ]));
    }

    Ok(())
}
