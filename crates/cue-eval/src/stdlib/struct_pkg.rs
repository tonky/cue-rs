use crate::value::{Value, ValueArena, ValueId};
use num_traits::ToPrimitive;

pub fn call_struct(
    arena: &mut ValueArena,
    func_name: &str,
    args: &[ValueId],
) -> Result<ValueId, String> {
    match ("struct", func_name) {
        // --- struct package ---
        ("struct", "MinFields") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(i)) = arena.get(arg0)
                && let Some(n) = i.to_usize()
            {
                let target = arena.alloc(Value::Int(n.into()));
                return Ok(arena.alloc(Value::BuiltinValidator {
                    name: format!("struct.MinFields({n})"),
                    target,
                }));
            }
            Err("struct.MinFields requires 1 positive integer argument".to_string())
        }
        ("struct", "MaxFields") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(i)) = arena.get(arg0)
                && let Some(n) = i.to_usize()
            {
                let target = arena.alloc(Value::Int(n.into()));
                return Ok(arena.alloc(Value::BuiltinValidator {
                    name: format!("struct.MaxFields({n})"),
                    target,
                }));
            }
            Err("struct.MaxFields requires 1 positive integer argument".to_string())
        }
        _ => Err(format!("unknown struct function: struct.{func_name}")),
    }
}
