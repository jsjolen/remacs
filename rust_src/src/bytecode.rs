//! Execution of byte code produced by bytecomp.el.

use remacs_macros::lisp_fn;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::mem;
use std::slice;
use std::vec::Vec;

use enum_primitive_derive::Primitive;
use num_traits::{FromPrimitive, ToPrimitive};

use crate::{
    eval::funcall, eval::unbind_to, hashtable::HashLookupResult::Found,
    hashtable::HashLookupResult::Missing, hashtable::HashTableIter, hashtable::KeyAndValueIter,
    hashtable::LispHashTableRef, lisp::LispObject, lists::list,
    remacs_sys::exec_byte_code as c_exec_byte_code, remacs_sys::handlertype,
    remacs_sys::make_number, remacs_sys::set_internal, remacs_sys::specbind, remacs_sys::xsignal2,
    remacs_sys::Fcons, remacs_sys::Flist, remacs_sys::Qnil, remacs_sys::Qt,
    remacs_sys::Qwrong_number_of_arguments, remacs_sys::Set_Internal_Bind, strings,
    threads::c_specpdl_index, threads::ThreadState,
};

// Temporary Rust wrapper for C's exec_byte_code
fn rust_exec_byte_code(
    bytestr: LispObject,
    vector: LispObject,
    maxdepth: LispObject,
    args_template: LispObject,
    arg: &mut [LispObject],
) -> LispObject {
    unsafe {
        c_exec_byte_code(
            bytestr,
            vector,
            maxdepth,
            args_template,
            arg.len() as isize,
            arg.as_mut_ptr() as *mut LispObject,
        )
    }
}

#[derive(Copy, Clone, Primitive)]
enum OpCodes {
    Stack_ref = 0, // Done
    Stack_ref1 = 1,
    Stack_ref2 = 2,
    Stack_ref3 = 3,
    Stack_ref4 = 4,
    Stack_ref5 = 5,
    Stack_ref6 = 6,
    Stack_ref7 = 7,
    Varref = 0o10,
    Varref1 = 0o11,
    Varref2 = 0o12,
    Varref3 = 0o13,
    Varref4 = 0o14,
    Varref5 = 0o15,
    Varref6 = 0o16,
    Varref7 = 0o17,
    Varset = 0o20,
    Varset1 = 0o21,
    Varset2 = 0o22,
    Varset3 = 0o23,
    Varset4 = 0o24,
    Varset5 = 0o25,
    Varset6 = 0o26,
    Varset7 = 0o27,
    Varbind = 0o30,
    Varbind1 = 0o31,
    Varbind2 = 0o32,
    Varbind3 = 0o33,
    Varbind4 = 0o34,
    Varbind5 = 0o35,
    Varbind6 = 0o36,
    Varbind7 = 0o37,
    Call = 0o40, // Done
    Call1 = 0o41,
    Call2 = 0o42,
    Call3 = 0o43,
    Call4 = 0o44,
    Call5 = 0o45,
    Call6 = 0o46,
    Call7 = 0o47,
    Unbind = 0o50,
    Unbind1 = 0o51,
    Unbind2 = 0o52,
    Unbind3 = 0o53,
    Unbind4 = 0o54,
    Unbind5 = 0o55,
    Unbind6 = 0o56,
    Unbind7 = 0o57,
    Pophandler = 0o60,
    Pushconditioncase = 0o61,
    Pushcatch = 0o62,
    Nth = 0o70,
    Symbolp = 0o71, // Done
    Consp = 0o72,   // Done
    Stringp = 0o73, // Done
    Listp = 0o74,   // Done
    Eq = 0o75,
    Memq = 0o76,
    Not = 0o77,
    Car = 0o100,
    Cdr = 0o101,
    Cons = 0o102,
    List1 = 0o103,
    List2 = 0o104,
    List3 = 0o105,
    List4 = 0o106,
    Length = 0o107,
    Aref = 0o110,
    Aset = 0o111,
    Symbol_value = 0o112,
    Symbol_function = 0o113,
    Set = 0o114,
    Fset = 0o115,
    Get = 0o116,
    Substring = 0o117,
    Concat2 = 0o120,
    Concat3 = 0o121,
    Concat4 = 0o122,
    Sub1 = 0o123,
    Add1 = 0o124,
    Eqlsign = 0o125,
    Gtr = 0o126,
    Lss = 0o127,
    Leq = 0o130,
    Geq = 0o131,
    Diff = 0o132,
    Negate = 0o133,
    Plus = 0o134,
    Max = 0o135,
    Min = 0o136,
    Mult = 0o137,
    Point = 0o140,
    Save_current_buffer = 0o141,
    Goto_char = 0o142,
    Insert = 0o143,
    Point_max = 0o144,
    Point_min = 0o145,
    Char_after = 0o146,
    Following_char = 0o147,
    Preceding_char = 0o150,
    Current_column = 0o151,
    Indent_to = 0o152,
    Eolp = 0o154,
    Eobp = 0o155,
    Bolp = 0o156,
    Bobp = 0o157,
    Current_buffer = 0o160,
    Set_buffer = 0o161,
    Save_current_buffer_1 = 0o162,
    Interactive_p = 0o164,
    Forward_char = 0o165,
    Forward_word = 0o166,
    Skip_chars_forward = 0o167,
    Skip_chars_backward = 0o170,
    Forward_line = 0o171,
    Char_syntax = 0o172,
    Buffer_substring = 0o173,
    Delete_region = 0o174,
    Narrow_to_region = 0o175,
    Widen = 0o176,
    End_of_line = 0o177,
    Constant2 = 0o201,
    Goto = 0o202,
    Gotoifnil = 0o203,
    Gotoifnonnil = 0o204,
    Gotoifnilelsepop = 0o205,
    Gotoifnonnilelsepop = 0o206,
    Return = 0o207,
    Discard = 0o210,
    Dup = 0o211,
    Save_excursion = 0o212,
    Save_window_excursion = 0o213,
    Save_restriction = 0o214,
    Catch = 0o215,
    Unwind_protect = 0o216,
    Condition_case = 0o217,
    Temp_output_buffer_setup = 0o220,
    Temp_output_buffer_show = 0o221,
    Unbind_all = 0o222,
    Set_marker = 0o223,
    Match_beginning = 0o224,
    Match_end = 0o225,
    Upcase = 0o226,
    Downcase = 0o227,
    Stringeqlsign = 0o230,
    Stringlss = 0o231,
    Equal = 0o232,
    Nthcdr = 0o233,
    Elt = 0o234,
    Member = 0o235,
    Assq = 0o236,
    Nreverse = 0o237,
    Setcar = 0o240,
    Setcdr = 0o241,
    Car_safe = 0o242,
    Cdr_safe = 0o243,
    Nconc = 0o244,
    Quo = 0o245,
    Rem = 0o246,
    Numberp = 0o247,
    Integerp = 0o250,
    RGoto = 0o252,
    RGotoifnil = 0o253,
    RGotoifnonnil = 0o254,
    RGotoifnilelsepop = 0o255,
    RGotoifnonnilelsepop = 0o256,
    ListN = 0o257,
    ConcatN = 0o260,
    InsertN = 0o261,
    Stack_set = 0o262,
    Stack_set2 = 0o263,
    DiscardN = 0o266,
    Switch = 0o267,
    Constant = 0o300,
}

fn exec_byte_code(
    bytestr: LispObject,
    vector: LispObject,
    maxdepth: LispObject,
    args_template: LispObject,
    args: &mut [LispObject],
) -> LispObject {
    // Incorrect types
    if !bytestr.is_string() {
    } else if !vector.is_vector() {
    } else if !maxdepth.is_natnum() {
    }

    // if (STRING_MULTIBYTE) ...
    // Deal with this later, as it only exists for really old backwards compatible code

    if args_template.is_not_nil() {
        // Deal with args_template
    }
    /*
    Evaluation set-up.
    bytecode.c sets up such that the operation stack and the operand stacks
    are contiguous and part of the same buffer.
    This is highly questionable :-).
    Maybe we can do the same allocation in Rust but split it such that we get some nice type-checking?
    */

    // Quote: lisp.h
    /* Elisp uses several stacks:
    - the C stack.
    - the bytecode stack: used internally by the bytecode interpreter.
      Allocated from the C stack.
    - The specpdl stack: keeps track of active unwind-protect and
      dynamic-let-bindings.  Allocated from the `specpdl' array, a manually
      managed stack.
    - The handler stack: keeps track of active catch tags and condition-case
      handlers.  Allocated in a manually managed stack implemented by a
    doubly-linked list allocated via xmalloc and never freed.  */

    let mut operandStack: Vec<LispObject> = match maxdepth.as_fixnum() {
        Some(i) if i >= 0 => Vec::with_capacity(i as usize),
        Some(_) => Vec::with_capacity(0),
        None => panic!("maxdepth must fit within fixnum"),
    };
    let constantVector = vector.as_vector_or_error();
    let bstr = bytestr.force_string();

    // Re-interpret bytestr as slice of u8s
    let bytecode: &[u8];
    unsafe {
        bytecode = slice::from_raw_parts(
            bstr.const_data_ptr(),
            // Why would the len_bytes() be inclusive on the sign -- really??
            mem::size_of::<u8>() * (usize::try_from(bstr.len_bytes()).unwrap()),
        );
    };
    format!(
        "Size of bytecode: {}; Size of bstr: {}",
        bytecode.len(),
        bstr.len_bytes()
    );

    let LARGE_NUMBER_MEANT_TO_BE_AS_LARGE_AS_PTRDIFF_MAX: usize = 99999999;
    if args_template.is_not_nil() {
        let nargs: usize = usize::try_from(args.len()).unwrap();
        let tmpl: libc::c_long = args_template.as_fixnum_coerce_marker_or_error();
        let rest = if (tmpl & 128) != 0 { 1 } else { 0 };
        let mandatory = usize::try_from(tmpl & 127).unwrap(); // 0-7th bits
                                                              // tmpl is i64 -- usize is as wide as i32 on 32-bit systems (therefore unwrap)
                                                              // It's a rather safe assumption that our bytecode won't be 2^32 elts long
        let nonrest: usize = usize::try_from(tmpl >> 8).unwrap(); // req+optional according to manual
        let maxargs = if rest == 1 {
            LARGE_NUMBER_MEANT_TO_BE_AS_LARGE_AS_PTRDIFF_MAX
        } else {
            nonrest
        };

        if !(mandatory <= nargs && nargs <= maxargs) {
            unsafe {
                xsignal2(
                    Qwrong_number_of_arguments,
                    Fcons(
                        make_number(i64::try_from(mandatory).unwrap()),
                        make_number(i64::try_from(nonrest).unwrap()),
                    ),
                    make_number(i64::try_from(nargs).unwrap()),
                );
            }
        }
        let pushedargs = if nonrest < nargs { nonrest } else { nargs };
        let mut idx: usize = 0;
        while idx < pushedargs {
            operandStack.push(args[idx]);
            idx = idx + 1;
        }
        if nonrest < nargs {
            let (_fst, snd) = args.split_at_mut(idx);
            operandStack.push(list(snd));
        }
    }

    let mut pc: usize = 0;
    let mut op: u8;

    loop {
        op = bytecode[pc];
        println!("{}", op);

        match OpCodes::from_u8(op) {
            None => {
                /**
                OpCodes::Constant is implemented here.
                Yes, it's an annoying special-case.
                 **/
                let opconst = OpCodes::Constant as usize;
                if (opconst <= usize::from(op) && usize::from(op) < opconst + constantVector.len())
                {
                    let i = usize::from(op - (OpCodes::Constant as u8));
                    operandStack.push(constantVector.get(i));
                    pc = pc + 1;
                }
            }
            Some(opcode) => {
                match opcode {
                    OpCodes::Stack_ref1
                    | OpCodes::Stack_ref2
                    | OpCodes::Stack_ref3
                    | OpCodes::Stack_ref4
                    | OpCodes::Stack_ref5 => {
                        let i = op - (OpCodes::Stack_ref as u8);
                        let v1: LispObject = operandStack[operandStack.len() - usize::from(i)];
                        operandStack.push(v1);
                        pc = pc + 1;
                    }

                    OpCodes::Stack_ref6 => {
                        let i = bytecode[pc + 1];
                        let v1: LispObject = operandStack[operandStack.len() - usize::from(i)];
                        operandStack.push(v1);
                        pc = pc + 2;
                    }
                    OpCodes::Stack_ref7 => {
                        let i = u16::from(bytecode[pc + 1]) + (u16::from(bytecode[pc + 2]) << 8);
                        let v1: LispObject = operandStack[operandStack.len() - usize::from(i)];
                        operandStack.push(v1);
                        pc = pc + 3;
                    }

                    /*
                    TODO:
                    The equiv. C-code has a very messed up if-stmt which I believe corresponds to symbol_value().
                     */
                    OpCodes::Varref
                    | OpCodes::Varref1
                    | OpCodes::Varref2
                    | OpCodes::Varref3
                    | OpCodes::Varref4
                    | OpCodes::Varref5 => {
                        let i = usize::from(op - (OpCodes::Varref as u8));
                        unsafe {
                            let v1: LispObject =
                                constantVector.get(i).as_symbol().unwrap().find_value();
                            operandStack.push(v1);
                        }
                        pc = pc + 1;
                    }
                    OpCodes::Varref6 => {
                        let i = usize::from(bytecode[pc + 1]);
                        unsafe {
                            let v1: LispObject =
                                constantVector.get(i).as_symbol().unwrap().find_value();
                            operandStack.push(v1);
                        }
                        pc = pc + 2;
                    }
                    OpCodes::Varref7 => {
                        let i = usize::from(
                            u16::from(bytecode[pc + 1]) + (u16::from(bytecode[pc + 2]) << 8),
                        );
                        unsafe {
                            let v1: LispObject =
                                constantVector.get(i).as_symbol().unwrap().find_value();
                            operandStack.push(v1);
                        }
                        pc = pc + 3;
                    }

                    // TODO: C code inlines most common use-case
                    // We skip this and go straight to the standard case.
                    OpCodes::Varset
                    | OpCodes::Varset1
                    | OpCodes::Varset2
                    | OpCodes::Varset3
                    | OpCodes::Varset4
                    | OpCodes::Varset5 => {
                        let i = usize::from(op - (OpCodes::Varset as u8));
                        let x = operandStack.pop().unwrap();
                        unsafe {
                            let v1: LispObject = constantVector.get(i);
                            set_internal(v1, x, Qnil, Set_Internal_Bind::SET_INTERNAL_SET)
                        }
                        pc = pc + 1;
                    }
                    OpCodes::Varset6 => {
                        let i = usize::from(bytecode[pc + 1]);
                        let x = operandStack.pop().unwrap();
                        unsafe {
                            let v1: LispObject = constantVector.get(i);
                            set_internal(v1, x, Qnil, Set_Internal_Bind::SET_INTERNAL_SET)
                        }
                        pc = pc + 2;
                    }
                    OpCodes::Varset7 => {
                        let i = usize::from(
                            u16::from(bytecode[pc + 1]) + (u16::from(bytecode[pc + 2]) << 8),
                        );
                        let x = operandStack.pop().unwrap();
                        unsafe {
                            let v1: LispObject = constantVector.get(i);
                            set_internal(v1, x, Qnil, Set_Internal_Bind::SET_INTERNAL_SET)
                        }
                        pc = pc + 3;
                    }

                    OpCodes::Varbind
                    | OpCodes::Varbind1
                    | OpCodes::Varbind2
                    | OpCodes::Varbind3
                    | OpCodes::Varbind4
                    | OpCodes::Varbind5 => {
                        let i = usize::from(op - (OpCodes::Varbind as u8));
                        let x = operandStack.pop().unwrap();
                        unsafe {
                            specbind(constantVector.get(i), x);
                        }
                        pc = pc + 1;
                    }
                    OpCodes::Varbind6 => {
                        let i = usize::from(bytecode[pc + 1]);
                        let x = operandStack.pop().unwrap();
                        unsafe {
                            specbind(constantVector.get(i), x);
                        }
                        pc = pc + 2;
                    }
                    OpCodes::Varbind7 => {
                        let i = usize::from(
                            u16::from(bytecode[pc + 1]) + (u16::from(bytecode[pc + 2]) << 8),
                        );
                        let x = operandStack.pop().unwrap();
                        unsafe {
                            specbind(constantVector.get(i), x);
                        }
                        pc = pc + 3;
                    }

                    OpCodes::Call
                    | OpCodes::Call1
                    | OpCodes::Call2
                    | OpCodes::Call3
                    | OpCodes::Call4
                    | OpCodes::Call5 => {
                        let argCount = usize::from(op - (OpCodes::Call as u8));
                        let len = operandStack.len();
                        let result = funcall(&mut operandStack[len - (argCount + 1)..]);
                        for _ in 0..(argCount + 1) {
                            operandStack.pop();
                        }
                        operandStack.push(result);
                        pc = pc + 1;
                    }
                    OpCodes::Call6 => {
                        let argCount = usize::from(bytecode[pc + 1]);
                        let len = operandStack.len();
                        let result = funcall(&mut operandStack[len - (argCount + 1)..]);
                        for _ in 0..(argCount + 1) {
                            operandStack.pop();
                        }
                        operandStack.push(result);
                        pc = pc + 2;
                    }
                    OpCodes::Call7 => {
                        let argCount = usize::from(
                            u16::from(bytecode[pc + 1]) + (u16::from(bytecode[pc + 2]) << 8),
                        );
                        let len = operandStack.len();
                        let result = funcall(&mut operandStack[len - (argCount + 1)..]);
                        for _ in 0..(argCount + 1) {
                            operandStack.pop();
                        }
                        operandStack.push(result);
                        pc = pc + 3;
                    }

                    OpCodes::Unbind
                    | OpCodes::Unbind1
                    | OpCodes::Unbind2
                    | OpCodes::Unbind3
                    | OpCodes::Unbind4
                    | OpCodes::Unbind5 => {
                        let i = isize::try_from(op - (OpCodes::Unbind as u8)).unwrap();
                        unbind_to(c_specpdl_index() - i, Qnil);
                        pc = pc + 1;
                    }
                    OpCodes::Unbind6 => {
                        let i = isize::try_from(bytecode[pc + 1]).unwrap();
                        unbind_to(c_specpdl_index() - i, Qnil);
                        pc = pc + 2;
                    }
                    OpCodes::Unbind7 => {
                        let i = isize::try_from(
                            u16::from(bytecode[pc + 1]) + (u16::from(bytecode[pc + 2]) << 8),
                        )
                        .unwrap();
                        unbind_to(c_specpdl_index() - i, Qnil);
                        pc = pc + 3;
                    }
                    // This is just if and only if constant == 0
                    OpCodes::Constant => {
                        let i = usize::from(op - (OpCodes::Constant as u8));
                        operandStack.push(constantVector.get(i));
                        pc = pc + 1;
                    }

                    OpCodes::Constant2 => {
                        let i = usize::from(
                            u16::from(bytecode[pc + 1]) + (u16::from(bytecode[pc + 2]) << 8),
                        );
                        operandStack.push(constantVector.get(i));
                        pc = pc + 3;
                    }

                    OpCodes::Pophandler => {
                        // In bytecode.c this exact code is used -- why does that not leak memory?
                        // Because push_handler_nosignal also keeps a reference around through nextfree, therefore doesn't leak.
                        unsafe {
                            ThreadState::current_thread().m_handlerlist =
                                (*ThreadState::current_thread().m_handlerlist).next;
                        }
                        pc = pc + 1;
                    }

                    // Needs to deal with very annoying stuff.
                    OpCodes::Pushconditioncase => {
                        /*
                                let i =
                                    usize::from(u16::from(bytecode[pc + 1]) + (u16::from(bytecode[pc + 2]) << 8));
                                let v1 = operandStack.pop();
                                // Mama mia.
                                unsafe {
                                    let c = ThreadState::push_handler(v1, handlertype::CONDITION_CASE);
                                    let top = operandStack.as_ptr().add(operandStack.len() - 1);
                                    c.bytecode_dest = i;
                                    c.bytecode_top = top;
                                    if (sys_setjmp(c.jmp)) {
                                        let c2 = ThreadState::current_thread().m_handlerlist;
                                        top = c2.bytecode_top;
                                        op = c2.bytecode_dest;
                                        ThreadState::current_thread().m_handlerlist = c2.next;
                                        operandStack.push(c.val);
                                        // goto op_branch
                                    }
                                }
                                pc = pc + 1;
                        */
                    }
                    OpCodes::Pushcatch => {}

                    OpCodes::Goto => {
                        let i = usize::from(
                            u16::from(bytecode[pc + 1]) + (u16::from(bytecode[pc + 2]) << 8),
                        );
                        pc = i;
                    }
                    OpCodes::Gotoifnil => {
                        let i = usize::from(
                            u16::from(bytecode[pc + 1]) + (u16::from(bytecode[pc + 2]) << 8),
                        );
                        let v = operandStack.pop().unwrap();
                        if v.is_nil() {
                            pc = i;
                        } else {
                            pc = pc + 3;
                        }
                    }
                    OpCodes::Gotoifnonnil => {
                        let i = usize::from(
                            u16::from(bytecode[pc + 1]) + (u16::from(bytecode[pc + 2]) << 8),
                        );
                        let v = operandStack.pop().unwrap();
                        if v.is_not_nil() {
                            pc = i;
                        } else {
                            pc = pc + 3;
                        }
                    }
                    OpCodes::Gotoifnilelsepop => {
                        let i = usize::from(
                            u16::from(bytecode[pc + 1]) + (u16::from(bytecode[pc + 2]) << 8),
                        );
                        let v = operandStack[operandStack.len() - 1];
                        if v.is_nil() {
                            pc = i;
                        } else {
                            operandStack.pop();
                            pc = pc + 3;
                        }
                    }
                    OpCodes::Gotoifnonnilelsepop => {
                        let i = usize::from(
                            u16::from(bytecode[pc + 1]) + (u16::from(bytecode[pc + 2]) << 8),
                        );
                        let v = operandStack[operandStack.len() - 1];
                        if v.is_not_nil() {
                            pc = i;
                        } else {
                            operandStack.pop();
                            pc = pc + 3;
                        }
                    }

                    OpCodes::Switch => {
                        let ht = LispHashTableRef::from(operandStack.pop().unwrap());
                        let key = operandStack.pop().unwrap();
                        // TODO: Perform linear search if |ht| <= 5. Replicates bytecode.c behavior.
                        /*
                                if ht.size() <= 5 {
                        for (k, v) in ht.iter() {
                        }
                                } else {
                                }*/
                        match ht.lookup(key) {
                            Missing(_) => {
                                pc = pc + 1;
                            }
                            Found(idx) => unsafe {
                                let i = usize::try_from(
                                    i64::try_from(ht.get_hash_value(idx).to_fixnum_unchecked())
                                        .unwrap(),
                                )
                                .unwrap();
                                pc = i;
                            },
                        }
                    }

                    OpCodes::Listp => {
                        let v = operandStack.pop().unwrap();
                        if v.is_list() {
                            operandStack.push(Qt);
                        } else {
                            operandStack.push(Qnil);
                        }
                        pc = pc + 1;
                    }
                    OpCodes::Symbolp => {
                        let v = operandStack.pop().unwrap();
                        if v.is_symbol() {
                            operandStack.push(Qt);
                        } else {
                            operandStack.push(Qnil);
                        }
                        pc = pc + 1;
                    }
                    OpCodes::Consp => {
                        let v = operandStack.pop().unwrap();
                        if v.is_cons() {
                            operandStack.push(Qt);
                        } else {
                            operandStack.push(Qnil);
                        }
                        pc = pc + 1;
                    }
                    OpCodes::Stringp => {
                        let v = operandStack.pop().unwrap();
                        if v.is_string() {
                            operandStack.push(Qt);
                        } else {
                            operandStack.push(Qnil);
                        }
                        pc = pc + 1;
                    }
                    OpCodes::Eq => {
                        let v1 = operandStack.pop().unwrap();
                        let v2 = operandStack.pop().unwrap();
                        operandStack.push(LispObject::from(v1.eq(v2)));
                        pc = pc + 1;
                    }
                    OpCodes::Equal => {
                        let v1 = operandStack.pop().unwrap();
                        let v2 = operandStack.pop().unwrap();
                        operandStack.push(LispObject::from(v1.equal(v2)));
                        pc = pc + 1;
                    }
                    OpCodes::Elt => {}

                    OpCodes::Return => {
                        return operandStack.pop().unwrap();
                    }

                    OpCodes::Dup => {
                        operandStack.push(*operandStack.last().unwrap());
                        pc = pc + 1;
                    }
                    _ => {
                        panic!(format!("Unimplemented: {}", op));
                    }
                }
            }
        }
    }
}

/// Function used internally in byte-compiled code.
/// The first argument, BYTESTR, is a string of byte code;
/// the second, VECTOR, a vector of constants;
/// the third, MAXDEPTH, the maximum stack depth used in this function.
/// If the third argument is incorrect, Emacs may crash :(
#[lisp_fn]
pub fn byte_code(bytestr: LispObject, vector: LispObject, maxdepth: LispObject) -> LispObject {
    rust_exec_byte_code(bytestr, vector, maxdepth, Qnil, &mut [])
}

#[lisp_fn]
pub fn rust_byte_code(bytestr: LispObject, vector: LispObject, maxdepth: LispObject) -> LispObject {
    exec_byte_code(bytestr, vector, maxdepth, Qnil, &mut [])
}

include!(concat!(env!("OUT_DIR"), "/bytecode_exports.rs"));
