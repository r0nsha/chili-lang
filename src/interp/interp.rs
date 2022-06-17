use super::{
    ffi::Ffi,
    lower::{Lower, LowerContext},
    vm::{
        display::dump_bytecode_to_file,
        instruction::{CompiledCode, Instruction},
        value::{Function, Value},
        Constants, Globals, VM,
    },
};
use crate::common::scopes::Scopes;
use crate::infer::ty_ctx::TyCtx;
use crate::{
    ast::{
        ast,
        ty::TyKind,
        workspace::{BindingInfoId, ModuleId, Workspace},
    },
    common::build_options::BuildOptions,
};
use std::collections::HashMap;
use ustr::{ustr, Ustr};

pub type InterpResult = Result<Value, InterpErr>;

#[derive(Debug)]
pub enum InterpErr {}

pub struct Interp {
    pub globals: Globals,
    pub constants: Constants,
    pub functions: HashMap<BindingInfoId, usize>,
    pub ffi: Ffi,
    pub build_options: BuildOptions,

    bindings_to_globals: HashMap<BindingInfoId, usize>,
}

impl Interp {
    pub fn new(build_options: BuildOptions) -> Self {
        Self {
            globals: vec![],
            constants: vec![Value::unit()],
            functions: HashMap::new(),
            ffi: Ffi::new(),
            build_options,
            bindings_to_globals: HashMap::new(),
        }
    }

    pub fn create_session<'i>(
        &'i mut self,
        workspace: &'i Workspace,
        tycx: &'i TyCtx,
        typed_ast: &'i ast::TypedAst,
    ) -> InterpSess<'i> {
        InterpSess {
            interp: self,
            workspace,
            tycx,
            typed_ast,
            env_stack: vec![],
            // labels: vec![],
            evaluated_globals: vec![],
        }
    }
}

pub struct InterpSess<'i> {
    pub interp: &'i mut Interp,
    pub workspace: &'i Workspace,
    pub tycx: &'i TyCtx,
    pub typed_ast: &'i ast::TypedAst,
    pub env_stack: Vec<(ModuleId, Env)>,

    // pub labels: Vec<Label>,

    // globals to be evaluated when the VM starts
    pub evaluated_globals: Vec<CompiledCode>,
}

// labels are used for patching call instruction after lowering
// pub struct Label {
//     instruction: *mut Instruction,
// }

pub type Env = Scopes<BindingInfoId, i16>;

impl<'i> InterpSess<'i> {
    pub fn eval(&'i mut self, expr: &ast::Expr, module_id: ModuleId) -> InterpResult {
        let verbose = self.workspace.build_options.verbose;
        let mut start_code = CompiledCode::new();

        self.env_stack.push((module_id, Env::default()));

        // lower expression tree into instructions
        expr.lower(self, &mut start_code, LowerContext { take_ptr: false });
        start_code.push(Instruction::Halt);

        let start_code = self.insert_init_instructions(start_code);

        self.env_stack.pop();

        if verbose {
            dump_bytecode_to_file(&self.interp.globals, &self.interp.constants, &start_code);
        }

        let mut vm = self.create_vm();

        let start_func = Function {
            id: BindingInfoId::unknown(),
            name: ustr("__vm_start"),
            arg_types: vec![],
            return_type: TyKind::Unit,
            code: start_code,
        };

        let result = vm.run_func(start_func);

        Ok(result)
    }

    // pushes initialization instructions such as global evaluation to the start
    fn insert_init_instructions(&mut self, mut code: CompiledCode) -> CompiledCode {
        let mut init_instructions: Vec<Instruction> = vec![];

        for (i, global_eval_code) in self.evaluated_globals.iter().enumerate() {
            let const_slot = self.interp.constants.len();
            self.interp.constants.push(Value::Function(Function {
                id: BindingInfoId::unknown(),
                name: ustr(&format!("global_init_{}", i)),
                arg_types: vec![],
                return_type: TyKind::Unit,
                code: global_eval_code.clone(),
            }));
            init_instructions.push(Instruction::PushConst(const_slot as u32));
            init_instructions.push(Instruction::Call(0));
        }

        code.instructions = init_instructions
            .into_iter()
            .chain(code.instructions)
            .collect();

        code
    }

    pub fn create_vm(&'i mut self) -> VM<'i> {
        VM::new(self.interp)
    }

    pub fn push_const(&mut self, code: &mut CompiledCode, value: Value) -> usize {
        let slot = self.interp.constants.len();
        self.interp.constants.push(value);
        code.push(Instruction::PushConst(slot as u32));
        slot
    }

    pub fn push_const_unit(&mut self, code: &mut CompiledCode) {
        // to avoid redundancy, when pushing a unit value,
        // we just use the first value in the constants vec
        code.push(Instruction::PushConst(0));
    }

    pub fn insert_global(&mut self, id: BindingInfoId, value: Value) -> usize {
        if let Some(&slot) = self.interp.bindings_to_globals.get(&id) {
            self.interp.globals[slot] = value;
            slot
        } else {
            let slot = self.interp.globals.len();
            self.interp.globals.push(value);
            self.interp.bindings_to_globals.insert(id, slot);
            slot
        }
    }

    pub fn get_global(&self, id: BindingInfoId) -> Option<usize> {
        self.interp.bindings_to_globals.get(&id).cloned()
    }

    #[allow(unused)]
    pub fn module_id(&self) -> ModuleId {
        self.env_stack.last().unwrap().0
    }

    pub fn env(&self) -> &Env {
        &self.env_stack.last().unwrap().1
    }

    pub fn env_mut(&mut self) -> &mut Env {
        &mut self.env_stack.last_mut().unwrap().1
    }

    pub fn find_symbol(&self, module_id: ModuleId, symbol: Ustr) -> BindingInfoId {
        self.workspace
            .binding_infos
            .iter()
            .position(|(_, info)| info.module_id == module_id && info.symbol == symbol)
            .map(BindingInfoId::from)
            .unwrap_or_else(|| {
                panic!(
                    "couldn't find member `{}` in module `{}`",
                    self.workspace.get_module_info(module_id).unwrap().name,
                    symbol
                )
            })
    }

    pub fn add_local(&mut self, code: &mut CompiledCode, id: BindingInfoId) {
        code.locals += 1;
        self.env_mut().insert(id, code.locals as i16);
    }
}