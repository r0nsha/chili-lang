use crate::{
    instruction::{CompiledCode, Instruction},
    interp::Interp,
    stack::Stack,
    value::{Func, Pointer, Value},
};
use colored::Colorize;
use std::fmt::Display;
use ustr::ustr;

mod cast;
mod index;

const FRAMES_MAX: usize = 64;
const STACK_MAX: usize = FRAMES_MAX * (std::u8::MAX as usize) + 1;

pub type Constants = Vec<Value>;
pub type Globals = Vec<Value>;

#[derive(Debug, Clone)]
struct CallFrame {
    func: Func,
    ip: usize,
    slot: usize,
}

impl CallFrame {
    fn new(func: Func, slot: usize) -> Self {
        Self { func, ip: 0, slot }
    }
}

impl Display for CallFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<{:06}\t{}>", self.ip, self.func.name,)
    }
}

macro_rules! binary_op {
    ($stack:expr, $op:tt) => {
        let b = $stack.pop();
        let a = $stack.pop();


        match (&a, &b) {
            (Value::I8(a), Value::I8(b)) => $stack.push(Value::I8(a $op b)),
            (Value::I16(a), Value::I16(b)) => $stack.push(Value::I16(a $op b)),
            (Value::I32(a), Value::I32(b)) => $stack.push(Value::I32(a $op b)),
            (Value::I64(a), Value::I64(b)) => $stack.push(Value::I64(a $op b)),
            (Value::Int(a), Value::Int(b)) => $stack.push(Value::Int(a $op b)),
            (Value::U8(a), Value::U8(b)) => $stack.push(Value::U8(a $op b)),
            (Value::U16(a), Value::U16(b)) => $stack.push(Value::U16(a $op b)),
            (Value::U32(a), Value::U32(b)) => $stack.push(Value::U32(a $op b)),
            (Value::U64(a), Value::U64(b)) => $stack.push(Value::U64(a $op b)),
            (Value::Uint(a), Value::Uint(b)) => $stack.push(Value::Uint(a $op b)),
            (Value::F32(a), Value::F32(b)) => $stack.push(Value::F32(a $op b)),
            (Value::F64(a), Value::F64(b)) => $stack.push(Value::F64(a $op b)),
            _=> panic!("invalid types in binary operation `{}` : `{}` and `{}`", stringify!($op), a ,b)
        }
    };
}

macro_rules! comp_op {
    ($stack:expr, $op:tt) => {
        let b = $stack.pop();
        let a = $stack.pop();

        match (&a, &b) {
            (Value::Bool(a), Value::Bool(b)) => $stack.push(Value::Bool(a $op b)),
            (Value::I8(a), Value::I8(b)) => $stack.push(Value::Bool(a $op b)),
            (Value::I16(a), Value::I16(b)) => $stack.push(Value::Bool(a $op b)),
            (Value::I32(a), Value::I32(b)) => $stack.push(Value::Bool(a $op b)),
            (Value::I64(a), Value::I64(b)) => $stack.push(Value::Bool(a $op b)),
            (Value::Int(a), Value::Int(b)) => $stack.push(Value::Bool(a $op b)),
            (Value::U8(a), Value::U8(b)) => $stack.push(Value::Bool(a $op b)),
            (Value::U16(a), Value::U16(b)) => $stack.push(Value::Bool(a $op b)),
            (Value::U32(a), Value::U32(b)) => $stack.push(Value::Bool(a $op b)),
            (Value::U64(a), Value::U64(b)) => $stack.push(Value::Bool(a $op b)),
            (Value::Uint(a), Value::Uint(b)) => $stack.push(Value::Bool(a $op b)),
            (Value::F32(a), Value::F32(b)) => $stack.push(Value::Bool(a $op b)),
            (Value::F64(a), Value::F64(b)) => $stack.push(Value::Bool(a $op b)),
            _ => panic!("invalid types in compare operation `{}` and `{}`", a ,b)
        }
    };
}

macro_rules! logic_op {
    ($stack: expr, $op: tt) => {
        let b = $stack.pop();
        let a = $stack.pop();

        $stack.push(Value::Bool(a.is_truthy() $op b.is_truthy()));
    };
}

pub(crate) struct VM<'vm> {
    interp: &'vm mut Interp,
    stack: Stack<Value, STACK_MAX>,
    frames: Stack<CallFrame, FRAMES_MAX>,
}

impl<'vm> VM<'vm> {
    pub(crate) fn new(interp: &'vm mut Interp) -> Self {
        Self {
            interp,
            stack: Stack::new(),
            frames: Stack::new(),
        }
    }

    pub(crate) fn run(&'vm mut self, code: CompiledCode) -> Value {
        self.push_frame(Func {
            name: ustr("__vm_start"),
            param_count: 0,
            code,
        });

        self.run_loop()
    }

    fn run_loop(&'vm mut self) -> Value {
        loop {
            let inst = self.code().instructions[self.frames.peek(0).ip];

            self.trace(&self.frames.peek(0).ip, &inst, TraceLevel::Minimal);

            self.frames.peek_mut().ip += 1;

            match inst {
                Instruction::Noop => (),
                Instruction::Pop => {
                    self.stack.pop();
                }
                Instruction::PushConst(addr) => {
                    self.stack.push(self.get_const(addr).clone());
                }
                Instruction::Add => {
                    binary_op!(self.stack, +);
                }
                Instruction::Sub => {
                    binary_op!(self.stack, -);
                }
                Instruction::Mul => {
                    binary_op!(self.stack, *);
                }
                Instruction::Div => {
                    binary_op!(self.stack, /);
                }
                Instruction::Rem => {
                    binary_op!(self.stack, %);
                }
                Instruction::Neg => match self.stack.pop() {
                    Value::Int(v) => self.stack.push(Value::Int(-v)),
                    value => panic!("invalid value {}", value),
                },
                Instruction::Not => {
                    let value = self.stack.pop();
                    self.stack.push(Value::Bool(!value.is_truthy()));
                }
                Instruction::Deref => match self.stack.pop() {
                    Value::Pointer(ptr) => {
                        let value = unsafe { ptr.deref() };
                        self.stack.push(value);
                    }
                    value => panic!("invalid value {}", value),
                },
                Instruction::Eq => {
                    comp_op!(self.stack, ==);
                }
                Instruction::Neq => {
                    comp_op!(self.stack, !=);
                }
                Instruction::Lt => {
                    comp_op!(self.stack, <);
                }
                Instruction::LtEq => {
                    comp_op!(self.stack, <=);
                }
                Instruction::Gt => {
                    comp_op!(self.stack, >);
                }
                Instruction::GtEq => {
                    comp_op!(self.stack, >=);
                }
                Instruction::And => {
                    logic_op!(self.stack, &&);
                }
                Instruction::Or => {
                    logic_op!(self.stack, ||);
                }
                Instruction::Jmp(addr) => {
                    self.jmp(addr);
                }
                Instruction::Jmpt(addr) => {
                    let value = self.stack.peek(0);
                    if value.is_truthy() {
                        self.jmp(addr);
                    }
                }
                Instruction::Jmpf(addr) => {
                    let value = self.stack.peek(0);
                    if !value.is_truthy() {
                        self.jmp(addr);
                    }
                }
                Instruction::Return => {
                    let frame = self.frames.pop();
                    let return_value = self.stack.pop();

                    if self.frames.is_empty() {
                        break return_value;
                    } else {
                        self.stack.truncate(frame.slot - frame.func.param_count);
                        self.stack.push(return_value);
                    }
                }
                Instruction::Call(arg_count) => {
                    let value = self.stack.peek(0).clone();
                    match value {
                        Value::Func(func) => self.push_frame(func),
                        Value::ForeignFunc(func) => {
                            self.stack.pop(); // this pops the actual foreign function

                            let mut values = (0..arg_count)
                                .into_iter()
                                .map(|_| self.stack.pop())
                                .collect::<Vec<Value>>();
                            values.reverse();

                            // TODO: call_foreign_func should return a `Value`
                            let result = unsafe { self.interp.ffi.call(func, values) };
                            self.stack.push(result);
                        }
                        _ => panic!("tried to call an uncallable value `{}`", value),
                    }
                }
                Instruction::GetGlobal(slot) => {
                    match self.interp.globals.get(slot as usize) {
                        Some(value) => self.stack.push(value.clone()),
                        None => panic!("undefined global `{}`", slot),
                    };
                }
                Instruction::GetGlobalPtr(slot) => {
                    match self.interp.globals.get_mut(slot as usize) {
                        Some(value) => self.stack.push(Value::Pointer(value.into())),
                        None => panic!("undefined global `{}`", slot),
                    };
                }
                Instruction::SetGlobal(slot) => {
                    let value = self.stack.pop();
                    self.interp.globals[slot as usize] = value;
                }
                Instruction::Peek(slot) => {
                    let slot = self.frames.peek(0).slot as isize + slot as isize;
                    let value = self.stack.get(slot as usize).clone();
                    self.stack.push(value);
                }
                Instruction::PeekPtr(slot) => {
                    let slot = self.frames.peek(0).slot as isize + slot as isize;
                    let value = self.stack.get_mut(slot as usize);
                    let value = Value::Pointer(value.into());
                    self.stack.push(value);
                }
                Instruction::SetLocal(slot) => {
                    let slot = self.frames.peek(0).slot as isize + slot as isize;
                    let value = self.stack.pop();
                    self.stack.set(slot as usize, value);
                }
                Instruction::Index => {
                    let index = self.stack.pop().into_uint();
                    let value = self.stack.pop();
                    self.index(value, index);
                }
                Instruction::IndexPtr => {
                    let index = self.stack.pop().into_uint();
                    let value = self.stack.pop();
                    self.index_ptr(value, index);
                }
                Instruction::ConstIndex(index) => {
                    let value = self.stack.pop();
                    self.index(value, index as usize);
                }
                Instruction::ConstIndexPtr(index) => {
                    let value = self.stack.pop();
                    self.index_ptr(value, index as usize);
                }
                Instruction::Assign => {
                    let lvalue = self.stack.pop().into_pointer();
                    let rvalue = self.stack.pop();
                    lvalue.write_value(rvalue);
                }
                Instruction::Cast(cast) => self.cast_inst(cast),
                Instruction::AggregateAlloc => self.stack.push(Value::Aggregate(vec![])),
                Instruction::AggregatePush => {
                    let value = self.stack.pop();
                    let aggregate = self.stack.peek_mut().as_aggregate_mut();
                    aggregate.push(value);
                }
                Instruction::Copy => {
                    let value = self.stack.peek(0).clone();
                    self.stack.push(value);
                }
                Instruction::Increment => {
                    let ptr = self.stack.pop().into_pointer();
                    unsafe {
                        match ptr {
                            Pointer::I8(v) => *v += 1,
                            Pointer::I16(v) => *v += 1,
                            Pointer::I32(v) => *v += 1,
                            Pointer::I64(v) => *v += 1,
                            Pointer::Int(v) => *v += 1,
                            Pointer::U8(v) => *v += 1,
                            Pointer::U16(v) => *v += 1,
                            Pointer::U32(v) => *v += 1,
                            Pointer::U64(v) => *v += 1,
                            Pointer::Uint(v) => *v += 1,
                            _ => panic!("invalid pointer in increment {:?}", ptr),
                        }
                    }
                }
                Instruction::Halt => break self.stack.pop(),
            }
        }
    }

    fn push_frame(&mut self, func: Func) {
        // let slot = self.stack.len().checked_sub(1).unwrap_or(0);
        let slot = self.stack.len();
        for _ in 0..func.code.locals {
            self.stack.push(Value::unit());
        }
        self.frames.push(CallFrame::new(func, slot));
    }

    fn code(&self) -> &CompiledCode {
        &self.func().code
    }

    fn func(&self) -> &Func {
        &self.frames.peek(0).func
    }

    fn get_const(&self, addr: u32) -> &Value {
        self.interp.constants.get(addr as usize).unwrap()
    }

    fn jmp(&mut self, offset: i32) {
        let new_ip = self.frames.peek_mut().ip as isize + offset as isize;
        self.frames.peek_mut().ip = new_ip as usize;
    }

    fn trace(&self, ip: &usize, inst: &Instruction, level: TraceLevel) {
        let stack_trace = match level {
            TraceLevel::Minimal => format!("Stack count: {}", self.stack.len()),
            TraceLevel::All => self.stack.trace(),
        };

        println!(
            "{:06}\t{}\n\t{}",
            ip,
            inst.to_string().bold(),
            stack_trace.blue()
        );
    }
}

#[allow(dead_code)]
enum TraceLevel {
    Minimal,
    All,
}
