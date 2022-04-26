use crate::{
    ffi::call_foreign_func,
    instruction::{Bytecode, Instruction},
    interp::Interp,
    stack::Stack,
    value::{Func, Value},
};
use colored::Colorize;
use std::fmt::Display;

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
    ($stack: expr, $op: tt) => {
        let b = $stack.pop();
        let a = $stack.pop();

        match (&a, &b) {
            (Value::Int(a), Value::Int(b)) => $stack.push(Value::Int(a $op b)),
            _=> panic!("invalid types in binary operation `{}` and`{}`", a ,b)
        }
    };
}

macro_rules! comp_op {
    ($stack: expr, $op: tt) => {
        let b = $stack.pop();
        let a = $stack.pop();

        match (&a, &b) {
            (Value::Int(a), Value::Int(b)) => $stack.push(Value::Bool(a $op b)),
            (Value::Bool(a), Value::Bool(b)) => $stack.push(Value::Bool(a $op b)),
            _ => panic!("invalid types incompare operation `{}` and `{}`", a ,b)
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

    pub(crate) fn run(&'vm mut self, code: Bytecode) -> Value {
        let function = Func {
            name: "root".to_string(),
            param_count: 0,
            code,
        };

        self.frames.push(CallFrame::new(function, 0));

        self.run_loop()
    }

    fn run_loop(&'vm mut self) -> Value {
        loop {
            let inst = self.code()[self.frames.peek(0).ip];

            // self.trace(&self.frames.peek(0).ip, &inst);

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
                    let b = self.stack.pop();
                    let a = self.stack.pop();

                    match (b, a) {
                        (Value::Int(b), Value::Int(a)) => {
                            if a == 0 {
                                panic!("divide by zero")
                            }

                            self.stack.push(Value::Int(b / a))
                        }
                        _ => panic!("invalid types in division"),
                    }
                }
                Instruction::Rem => {
                    binary_op!(self.stack, %);
                }
                Instruction::Neg => match self.stack.pop() {
                    Value::Int(v) => self.stack.push(Value::Int(-v)),
                    _ => panic!("invalid type in neg"),
                },
                Instruction::Not => {
                    let value = self.stack.pop();
                    self.stack.push(Value::Bool(!value.is_truthy()));
                }
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
                    let value = self.stack.peek(0);
                    match value {
                        Value::Func(func) => {
                            let frame = CallFrame::new(func.clone(), self.stack.len() - 1);
                            self.frames.push(frame);
                        }
                        Value::ForeignFunc(func) => {
                            let func = func.clone();

                            self.stack.pop(); // this pops the actual foreign function

                            let mut values = (0..arg_count)
                                .into_iter()
                                .map(|_| self.stack.pop())
                                .collect::<Vec<Value>>();
                            values.reverse();

                            // TODO: call_foreign_func should return a `Value`
                            let result = unsafe { call_foreign_func(func, values) };
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
                        Some(value) => self.stack.push(Value::ValuePtr(value as *mut Value)),
                        None => panic!("undefined global `{}`", slot),
                    };
                }
                Instruction::SetGlobal(slot) => {
                    self.interp.globals.insert(slot as usize, self.stack.pop());
                }
                Instruction::GetLocal(slot) => {
                    let slot = self.frames.peek(0).slot as isize + slot as isize;
                    let value = self.stack.get(slot as usize).clone();
                    self.stack.push(value);
                }
                Instruction::GetLocalPtr(slot) => {
                    let slot = self.frames.peek(0).slot as isize + slot as isize;
                    let value = self.stack.get_mut(slot as usize) as *mut Value;
                    self.stack.push(Value::ValuePtr(value));
                }
                Instruction::SetLocal(slot) => {
                    let slot = self.frames.peek(0).slot as isize + slot as isize;
                    let value = self.stack.peek(0).clone();
                    self.stack.set(slot as usize, value);
                }
                // Instruction::Access(member) => {
                //     // TODO: in Assign context, i need to return the slot, not the member itself
                //     todo!("access")
                // }
                Instruction::Index(index) => {
                    let value = self.stack.pop();

                    match value {
                        Value::Tuple(elements) => self.stack.push(elements[index as usize].clone()),
                        Value::Slice(slice) => match index {
                            0 => self.stack.push(Value::Ptr(slice.ptr)),
                            1 => self.stack.push(Value::Int(slice.len as _)),
                            _ => panic!("invalid index {}", index),
                        },
                        _ => panic!("invalid value {}", value),
                    }
                }
                Instruction::Assign => {
                    let lvalue = self.stack.pop();
                    let rvalue = self.stack.pop();

                    match lvalue {
                        Value::Ptr(_) => todo!(),
                        Value::ValuePtr(ptr) => unsafe { *ptr = rvalue },
                        _ => panic!("invalid lvalue {}", lvalue),
                    }
                }
                Instruction::Halt => break self.stack.pop(),
            }
        }
    }

    fn code(&self) -> &Bytecode {
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

    fn trace(&self, ip: &usize, inst: &Instruction) {
        let stack_trace = self.stack.trace();

        println!(
            "{:06}\t{}\n\t{}",
            ip,
            inst.to_string().bold(),
            stack_trace.blue()
        );
    }
}
