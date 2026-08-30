use crate::value::{Value, ValueArena, ValueId};
use num_traits::{Signed, ToPrimitive};

pub fn call_math(
    arena: &mut ValueArena,
    pkg: &str,
    func_name: &str,
    args: &[ValueId],
) -> Result<ValueId, String> {
    match (pkg, func_name) {
// --- math package ---
        ("math", "Floor") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.floor()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(f) = i.to_f64() {
                        return Ok(arena.float(f.floor()));
                    }
            }
            Err("math.Floor requires 1 number argument".to_string())
        }
        ("math", "Ceil") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.ceil()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(f) = i.to_f64() {
                        return Ok(arena.float(f.ceil()));
                    }
            }
            Err("math.Ceil requires 1 number argument".to_string())
        }
        ("math", "Round") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.round()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(f) = i.to_f64() {
                        return Ok(arena.float(f.round()));
                    }
            }
            Err("math.Round requires 1 number argument".to_string())
        }
        ("math", "RoundToEven") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.round_ties_even()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(f) = i.to_f64() {
                        return Ok(arena.float(f.round_ties_even()));
                    }
            }
            Err("math.RoundToEven requires 1 number argument".to_string())
        }
        ("math", "Logb") => {
            if let Some(&arg0) = args.first() {
                let f_opt = match arena.get(arg0) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let Some(f) = f_opt {
                    if f == 0.0 {
                        return Ok(arena.float(f64::NEG_INFINITY));
                    }
                    return Ok(arena.float(f.abs().log2().floor()));
                }
            }
            Err("math.Logb requires 1 number argument".to_string())
        }
        ("math", "Ilogb") => {
            if let Some(&arg0) = args.first() {
                let f_opt = match arena.get(arg0) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let Some(f) = f_opt {
                    if f == 0.0 {
                        return Ok(arena.int(i32::MIN as i64));
                    }
                    return Ok(arena.int(f.abs().log2().floor() as i64));
                }
            }
            Err("math.Ilogb requires 1 number argument".to_string())
        }
        ("math", "Nextafter") => {
            if args.len() >= 2 {
                let x_opt = match arena.get(args[0]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                let y_opt = match arena.get(args[1]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let (Some(x), Some(y)) = (x_opt, y_opt) {
                    return Ok(arena.float(next_after(x, y)));
                }
            }
            Err("math.Nextafter requires 2 number arguments".to_string())
        }
        ("math", "FMA") => {
            if args.len() >= 3 {
                let x_opt = match arena.get(args[0]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                let y_opt = match arena.get(args[1]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                let z_opt = match arena.get(args[2]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let (Some(x), Some(y), Some(z)) = (x_opt, y_opt, z_opt) {
                    return Ok(arena.float(x.mul_add(y, z)));
                }
            }
            Err("math.FMA requires (x, y, z) number arguments".to_string())
        }
        ("math", "Pow10") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(i)) = arena.get(arg0)
                && let Some(n) = i.to_i32() {
                    return Ok(arena.float(10.0f64.powi(n)));
                }
            Err("math.Pow10 requires 1 integer argument".to_string())
        }
        ("math", "Scaleb") => {
            if args.len() >= 2 {
                let x_opt = match arena.get(args[0]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                let n_opt = match arena.get(args[1]) {
                    Some(Value::Int(i)) => i.to_i32(),
                    _ => None,
                };
                if let (Some(x), Some(n)) = (x_opt, n_opt) {
                    return Ok(arena.float(x * 2.0f64.powi(n)));
                }
            }
            Err("math.Scaleb requires (x number, n int) arguments".to_string())
        }
        ("math", "Frexp") => {
            if let Some(&arg0) = args.first() {
                let f_opt = match arena.get(arg0) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let Some(f) = f_opt {
                    let (frac, exp) = frexp_f64(f);
                    let frac_id = arena.float(frac);
                    let exp_id = arena.int(exp as i64);
                    return Ok(arena.alloc(Value::List {
                        elements: vec![frac_id, exp_id],
                        ellipsis: None,
                    }));
                }
            }
            Err("math.Frexp requires 1 number argument".to_string())
        }
        ("math", "Modf") => {
            if let Some(&arg0) = args.first() {
                let f_opt = match arena.get(arg0) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let Some(f) = f_opt {
                    let int_part = f.trunc();
                    let frac_part = f.fract();
                    let int_id = arena.float(int_part);
                    let frac_id = arena.float(frac_part);
                    return Ok(arena.alloc(Value::List {
                        elements: vec![int_id, frac_id],
                        ellipsis: None,
                    }));
                }
            }
            Err("math.Modf requires 1 number argument".to_string())
        }
        ("math", "Erf") => {
            if let Some(&arg0) = args.first() {
                let f_opt = match arena.get(arg0) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let Some(f) = f_opt {
                    return Ok(arena.float(erf_approx(f)));
                }
            }
            Err("math.Erf requires 1 number argument".to_string())
        }
        ("math", "Erfc") => {
            if let Some(&arg0) = args.first() {
                let f_opt = match arena.get(arg0) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let Some(f) = f_opt {
                    return Ok(arena.float(1.0 - erf_approx(f)));
                }
            }
            Err("math.Erfc requires 1 number argument".to_string())
        }
        ("math", "Gamma") => {
            if let Some(&arg0) = args.first() {
                let f_opt = match arena.get(arg0) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let Some(f) = f_opt {
                    return Ok(arena.float(gamma_approx(f)));
                }
            }
            Err("math.Gamma requires 1 number argument".to_string())
        }
        ("math", "LogGamma") => {
            if let Some(&arg0) = args.first() {
                let f_opt = match arena.get(arg0) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let Some(f) = f_opt {
                    return Ok(arena.float(gamma_approx(f).abs().ln()));
                }
            }
            Err("math.LogGamma requires 1 number argument".to_string())
        }
        ("math", "Trunc") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.trunc()));
                } else if let Some(Value::Int(i)) = arena.get(arg0) {
                    return Ok(arena.int(i.clone()));
                }
            }
            Err("math.Trunc requires 1 number argument".to_string())
        }
        ("math", "Abs") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.abs()));
                } else if let Some(Value::Int(i)) = arena.get(arg0) {
                    return Ok(arena.int(i.abs()));
                }
            }
            Err("math.Abs requires 1 number argument".to_string())
        }
        ("math", "Sign") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    let s = if *f > 0.0 { 1 } else if *f < 0.0 { -1 } else { 0 };
                    return Ok(arena.int(s));
                } else if let Some(Value::Int(i)) = arena.get(arg0) {
                    let s = match i.sign() {
                        num_bigint::Sign::Plus => 1,
                        num_bigint::Sign::NoSign => 0,
                        num_bigint::Sign::Minus => -1,
                    };
                    return Ok(arena.int(s));
                }
            }
            Err("math.Sign requires 1 number argument".to_string())
        }
        ("math", "Dim") => {
            if args.len() >= 2 {
                let x = match arena.get(args[0]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                let y = match arena.get(args[1]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let (Some(x_val), Some(y_val)) = (x, y) {
                    return Ok(arena.float((x_val - y_val).max(0.0)));
                }
            }
            Err("math.Dim requires 2 number arguments (x, y)".to_string())
        }
        ("math", "Copysign") => {
            if args.len() >= 2 {
                let x = match arena.get(args[0]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                let y = match arena.get(args[1]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let (Some(x_val), Some(y_val)) = (x, y) {
                    return Ok(arena.float(x_val.copysign(y_val)));
                }
            }
            Err("math.Copysign requires 2 number arguments (x, y)".to_string())
        }
        ("math", "Tan") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.tan()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(f) = i.to_f64() {
                        return Ok(arena.float(f.tan()));
                    }
            }
            Err("math.Tan requires 1 number argument".to_string())
        }
        ("math", "Asin") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.asin()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(f) = i.to_f64() {
                        return Ok(arena.float(f.asin()));
                    }
            }
            Err("math.Asin requires 1 number argument".to_string())
        }
        ("math", "Acos") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.acos()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(f) = i.to_f64() {
                        return Ok(arena.float(f.acos()));
                    }
            }
            Err("math.Acos requires 1 number argument".to_string())
        }
        ("math", "Atan") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.atan()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(f) = i.to_f64() {
                        return Ok(arena.float(f.atan()));
                    }
            }
            Err("math.Atan requires 1 number argument".to_string())
        }
        ("math", "Atan2") => {
            if args.len() >= 2 {
                let y = match arena.get(args[0]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                let x = match arena.get(args[1]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let (Some(y_val), Some(x_val)) = (y, x) {
                    return Ok(arena.float(y_val.atan2(x_val)));
                }
            }
            Err("math.Atan2 requires 2 number arguments (y, x)".to_string())
        }
        ("math", "Sqrt") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.sqrt()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_f64() {
                        return Ok(arena.float(n.sqrt()));
                    }
            }
            Err("math.Sqrt requires 1 number argument".to_string())
        }
        ("math", "Pow") => {
            if args.len() >= 2 {
                let base = match arena.get(args[0]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                let exp = match arena.get(args[1]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let (Some(b), Some(e)) = (base, exp) {
                    return Ok(arena.float(b.powf(e)));
                }
            }
            Err("math.Pow requires 2 number arguments (base, exp)".to_string())
        }
        ("math", "Log") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.ln()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_f64() {
                        return Ok(arena.float(n.ln()));
                    }
            }
            Err("math.Log requires 1 number argument".to_string())
        }
        ("math", "Log10") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.log10()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_f64() {
                        return Ok(arena.float(n.log10()));
                    }
            }
            Err("math.Log10 requires 1 number argument".to_string())
        }
        ("math", "Log2") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.log2()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_f64() {
                        return Ok(arena.float(n.log2()));
                    }
            }
            Err("math.Log2 requires 1 number argument".to_string())
        }
        ("math", "Hypot") => {
            if args.len() >= 2 {
                let p = match arena.get(args[0]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                let q = match arena.get(args[1]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let (Some(p_val), Some(q_val)) = (p, q) {
                    return Ok(arena.float(p_val.hypot(q_val)));
                }
            }
            Err("math.Hypot requires 2 number arguments (p, q)".to_string())
        }
        ("math", "Cbrt") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.cbrt()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(f) = i.to_f64() {
                        return Ok(arena.float(f.cbrt()));
                    }
            }
            Err("math.Cbrt requires 1 number argument".to_string())
        }
        ("math", "Exp") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.exp()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(f) = i.to_f64() {
                        return Ok(arena.float(f.exp()));
                    }
            }
            Err("math.Exp requires 1 number argument".to_string())
        }
        ("math", "Exp2") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.exp2()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(f) = i.to_f64() {
                        return Ok(arena.float(f.exp2()));
                    }
            }
            Err("math.Exp2 requires 1 number argument".to_string())
        }
        ("math", "Expm1") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.exp_m1()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(f) = i.to_f64() {
                        return Ok(arena.float(f.exp_m1()));
                    }
            }
            Err("math.Expm1 requires 1 number argument".to_string())
        }
        ("math", "Log1p") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.ln_1p()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(f) = i.to_f64() {
                        return Ok(arena.float(f.ln_1p()));
                    }
            }
            Err("math.Log1p requires 1 number argument".to_string())
        }
        ("math", "Remainder") => {
            if args.len() >= 2 {
                let x = match arena.get(args[0]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                let y = match arena.get(args[1]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let (Some(xv), Some(yv)) = (x, y) {
                    return Ok(arena.float(xv % yv));
                }
            }
            Err("math.Remainder requires 2 number arguments".to_string())
        }
        ("math", "Mod") => {
            if args.len() >= 2 {
                let x = match arena.get(args[0]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                let y = match arena.get(args[1]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let (Some(xv), Some(yv)) = (x, y) {
                    return Ok(arena.float(xv % yv));
                }
            }
            Err("math.Mod requires 2 number arguments".to_string())
        }
        ("math", "Ldexp") => {
            if args.len() >= 2 {
                let frac = match arena.get(args[0]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                let exp = match arena.get(args[1]) {
                    Some(Value::Int(i)) => i.to_i32(),
                    _ => None,
                };
                if let (Some(f), Some(e)) = (frac, exp) {
                    return Ok(arena.float(f * 2f64.powi(e)));
                }
            }
            Err("math.Ldexp requires (frac float, exp int) arguments".to_string())
        }
        ("math", "IsNaN") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.bool(f.is_nan()));
                } else if let Some(Value::Int(_)) = arena.get(arg0) {
                    return Ok(arena.bool(false));
                }
            }
            Err("math.IsNaN requires 1 number argument".to_string())
        }
        ("math", "IsInf") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.bool(f.is_infinite()));
                } else if let Some(Value::Int(_)) = arena.get(arg0) {
                    return Ok(arena.bool(false));
                }
            }
            Err("math.IsInf requires 1 number argument".to_string())
        }
        ("math", "Sin") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.sin()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_f64() {
                        return Ok(arena.float(n.sin()));
                    }
            }
            Err("math.Sin requires 1 number argument".to_string())
        }
        ("math", "Cos") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.cos()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_f64() {
                        return Ok(arena.float(n.cos()));
                    }
            }
            Err("math.Cos requires 1 number argument".to_string())
        }
        ("math", "Sinh") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.sinh()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_f64() {
                        return Ok(arena.float(n.sinh()));
                    }
            }
            Err("math.Sinh requires 1 number argument".to_string())
        }
        ("math", "Cosh") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.cosh()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_f64() {
                        return Ok(arena.float(n.cosh()));
                    }
            }
            Err("math.Cosh requires 1 number argument".to_string())
        }
        ("math", "Tanh") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.tanh()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_f64() {
                        return Ok(arena.float(n.tanh()));
                    }
            }
            Err("math.Tanh requires 1 number argument".to_string())
        }
        ("math", "Asinh") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.asinh()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_f64() {
                        return Ok(arena.float(n.asinh()));
                    }
            }
            Err("math.Asinh requires 1 number argument".to_string())
        }
        ("math", "Acosh") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.acosh()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_f64() {
                        return Ok(arena.float(n.acosh()));
                    }
            }
            Err("math.Acosh requires 1 number argument".to_string())
        }
        ("math", "Atanh") => {
            if let Some(&arg0) = args.first() {
                if let Some(Value::Float(f)) = arena.get(arg0) {
                    return Ok(arena.float(f.atanh()));
                } else if let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_f64() {
                        return Ok(arena.float(n.atanh()));
                    }
            }
            Err("math.Atanh requires 1 number argument".to_string())
        }
        ("math", "Max") => {
            if args.len() >= 2 {
                match (arena.get(args[0]), arena.get(args[1])) {
                    (Some(Value::Int(a)), Some(Value::Int(b))) => {
                        return Ok(arena.alloc(Value::Int(a.clone().max(b.clone()))));
                    }
                    (Some(Value::Float(a)), Some(Value::Float(b))) => {
                        return Ok(arena.float(a.max(*b)));
                    }
                    _ => {}
                }
            }
            Err("math.Max requires 2 comparable numbers".to_string())
        }
        ("math", "Min") => {
            if args.len() >= 2 {
                match (arena.get(args[0]), arena.get(args[1])) {
                    (Some(Value::Int(a)), Some(Value::Int(b))) => {
                        return Ok(arena.alloc(Value::Int(a.clone().min(b.clone()))));
                    }
                    (Some(Value::Float(a)), Some(Value::Float(b))) => {
                        return Ok(arena.float(a.min(*b)));
                    }
                    _ => {}
                }
            }
            Err("math.Min requires 2 comparable numbers".to_string())
        }
        ("math", "Pi") => Ok(arena.float(std::f64::consts::PI)),
        ("math", "E") => Ok(arena.float(std::f64::consts::E)),
        ("math", "Phi") => Ok(arena.float(1.618_033_988_749_895)),
        ("math", "Sqrt2") => Ok(arena.float(std::f64::consts::SQRT_2)),
        ("math", "SqrtE") => Ok(arena.float(std::f64::consts::E.sqrt())),
        ("math", "SqrtPi") => Ok(arena.float(std::f64::consts::PI.sqrt())),
        ("math", "SqrtPhi") => Ok(arena.float(1.618_033_988_749_895f64.sqrt())),
        ("math", "Ln2") => Ok(arena.float(std::f64::consts::LN_2)),
        ("math", "Log2E") => Ok(arena.float(std::f64::consts::LOG2_E)),
        ("math", "Ln10") => Ok(arena.float(std::f64::consts::LN_10)),
        ("math", "Log10E") => Ok(arena.float(std::f64::consts::LOG10_E)),
        ("math", "MultipleOf") => {
            if args.len() >= 2 {
                let v0 = match arena.get(args[0]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                let v1 = match arena.get(args[1]) {
                    Some(Value::Float(f)) => Some(*f),
                    Some(Value::Int(i)) => i.to_f64(),
                    _ => None,
                };
                if let (Some(a), Some(b)) = (v0, v1) {
                    if b == 0.0 {
                        return Err("error in call to math.MultipleOf: division by zero".to_string());
                    }
                    let rem = (a / b).round();
                    let diff = (a - rem * b).abs();
                    return Ok(arena.bool(diff < 1e-9));
                }
            }
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(i)) = arena.get(arg0)
                && let Some(n) = i.to_i64() {
                    let target = arena.alloc(Value::Int(n.into()));
                    return Ok(arena.alloc(Value::BuiltinValidator {
                        name: format!("math.MultipleOf({n})"),
                        target,
                    }));
                }
            Err("math.MultipleOf requires 1 or 2 number arguments".to_string())
        }

        // --- math/bits & bits package ---
        ("bits" | "math/bits", "And") => {
            if args.len() >= 2
                && let (Some(Value::Int(a)), Some(Value::Int(b))) = (arena.get(args[0]), arena.get(args[1]))
                && let (Some(a_i), Some(b_i)) = (a.to_i64(), b.to_i64()) {
                    return Ok(arena.int(a_i & b_i));
                }
            Err("bits.And requires 2 integer arguments".to_string())
        }
        ("bits" | "math/bits", "Or") => {
            if args.len() >= 2
                && let (Some(Value::Int(a)), Some(Value::Int(b))) = (arena.get(args[0]), arena.get(args[1]))
                && let (Some(a_i), Some(b_i)) = (a.to_i64(), b.to_i64()) {
                    return Ok(arena.int(a_i | b_i));
                }
            Err("bits.Or requires 2 integer arguments".to_string())
        }
        ("bits" | "math/bits", "Xor") => {
            if args.len() >= 2
                && let (Some(Value::Int(a)), Some(Value::Int(b))) = (arena.get(args[0]), arena.get(args[1]))
                && let (Some(a_i), Some(b_i)) = (a.to_i64(), b.to_i64()) {
                    return Ok(arena.int(a_i ^ b_i));
                }
            Err("bits.Xor requires 2 integer arguments".to_string())
        }
        ("bits" | "math/bits", "Lsh") => {
            if args.len() >= 2
                && let (Some(Value::Int(x)), Some(Value::Int(n))) = (arena.get(args[0]), arena.get(args[1]))
                && let (Some(x_i), Some(n_u)) = (x.to_i64(), n.to_u32()) {
                    return Ok(arena.int(x_i << n_u));
                }
            Err("bits.Lsh requires 1 integer and 1 shift count argument".to_string())
        }
        ("bits" | "math/bits", "Rsh") => {
            if args.len() >= 2
                && let (Some(Value::Int(x)), Some(Value::Int(n))) = (arena.get(args[0]), arena.get(args[1]))
                && let (Some(x_i), Some(n_u)) = (x.to_i64(), n.to_u32()) {
                    return Ok(arena.int(x_i >> n_u));
                }
            Err("bits.Rsh requires 1 integer and 1 shift count argument".to_string())
        }
        ("bits" | "math/bits", "OnesCount") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(x)) = arena.get(arg0)
                && let Some(x_u) = x.to_u64() {
                    return Ok(arena.int(x_u.count_ones() as i64));
                }
            Err("bits.OnesCount requires 1 non-negative integer argument".to_string())
        }
        ("bits" | "math/bits", "Len") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(x)) = arena.get(arg0)
                && let Some(x_u) = x.to_u64() {
                    let len = if x_u == 0 { 0 } else { 64 - x_u.leading_zeros() };
                    return Ok(arena.int(len as i64));
                }
            Err("bits.Len requires 1 non-negative integer argument".to_string())
        }
        ("bits" | "math/bits", "LeadingZeros") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(x)) = arena.get(arg0)
                && let Some(x_u) = x.to_u64() {
                    return Ok(arena.int(x_u.leading_zeros() as i64));
                }
            Err("bits.LeadingZeros requires 1 non-negative integer argument".to_string())
        }
        ("bits" | "math/bits", "TrailingZeros") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(x)) = arena.get(arg0)
                && let Some(x_u) = x.to_u64() {
                    return Ok(arena.int(x_u.trailing_zeros() as i64));
                }
            Err("bits.TrailingZeros requires 1 non-negative integer argument".to_string())
        }
        ("bits" | "math/bits", "Reverse") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(x)) = arena.get(arg0)
                && let Some(x_u) = x.to_u64() {
                    return Ok(arena.int(x_u.reverse_bits() as i64));
                }
            Err("bits.Reverse requires 1 non-negative integer argument".to_string())
        }
        _ => Err(format!("unknown math function: {pkg}.{func_name}")),
    }
}

fn next_after(x: f64, y: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    if x == y {
        return y;
    }
    if x == 0.0 {
        let smallest = f64::from_bits(1);
        return if y > 0.0 { smallest } else { -smallest };
    }
    let bits = x.to_bits();
    let next_bits = if (x > 0.0) == (y > x) {
        bits + 1
    } else {
        bits - 1
    };
    f64::from_bits(next_bits)
}

fn frexp_f64(f: f64) -> (f64, i32) {
    if f == 0.0 {
        return (0.0, 0);
    }
    let bits = f.to_bits();
    let exp_bits = ((bits >> 52) & 0x7FF) as i32;
    if exp_bits == 0 {
        // subnormal
        let (frac, exp) = frexp_f64(f * (1u64 << 54) as f64);
        return (frac, exp - 54);
    }
    if exp_bits == 0x7FF {
        return (f, 0);
    }
    let exp = exp_bits - 1022;
    let frac_bits = (bits & !(0x7FFu64 << 52)) | (1022u64 << 52);
    (f64::from_bits(frac_bits), exp)
}

fn erf_approx(x: f64) -> f64 {
    // Abramowitz and Stegun formula 7.1.26
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x_abs = x.abs();
    let p = 0.3275911;
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let t = 1.0 / (1.0 + p * x_abs);
    let poly = t * (a1 + t * (a2 + t * (a3 + t * (a4 + t * a5))));
    let y = 1.0 - poly * (-x_abs * x_abs).exp();
    sign * y
}

fn gamma_approx(z: f64) -> f64 {
    // Lanczos approximation for Gamma function (g=7, n=9)
    if z < 0.5 {
        std::f64::consts::PI / ((std::f64::consts::PI * z).sin() * gamma_approx(1.0 - z))
    } else {
        let z = z - 1.0;
        let p = [
            0.999_999_999_999_809_9,
            676.520_368_121_885_1,
            -1_259.139_216_722_402_8,
            771.323_428_777_653_1,
            -176.615_029_162_140_6,
            12.507_343_278_686_905,
            -0.138_571_095_836_526_25,
            9.984_369_578_019_572e-6,
            1.505_632_735_149_311_6e-7,
        ];
        let mut x = p[0];
        for (i, &p_val) in p.iter().enumerate().skip(1) {
            x += p_val / (z + i as f64);
        }
        let t = z + 7.5;
        (2.0 * std::f64::consts::PI).sqrt() * t.powf(z + 0.5) * (-t).exp() * x
    }
}
