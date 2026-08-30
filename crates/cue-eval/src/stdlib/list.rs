use crate::value::{Value, ValueArena, ValueId};
use num_traits::ToPrimitive;
use std::collections::HashSet;

pub fn call_list(
    arena: &mut ValueArena,
    func_name: &str,
    args: &[ValueId],
) -> Result<ValueId, String> {
    match ("list", func_name) {
// --- list package ---
        ("list", "MinItems") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_usize() {
                        let target = arena.alloc(Value::Int(n.into()));
                        return Ok(arena.alloc(Value::BuiltinValidator {
                            name: format!("list.MinItems({n})"),
                            target,
                        }));
                    }
            Err("list.MinItems requires 1 positive integer argument".to_string())
        }
        ("list", "MaxItems") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::Int(i)) = arena.get(arg0)
                    && let Some(n) = i.to_usize() {
                        let target = arena.alloc(Value::Int(n.into()));
                        return Ok(arena.alloc(Value::BuiltinValidator {
                            name: format!("list.MaxItems({n})"),
                            target,
                        }));
                    }
            Err("list.MaxItems requires 1 positive integer argument".to_string())
        }
        ("list", "UniqueItems") => {
            let dummy = arena.alloc(Value::Top);
            Ok(arena.alloc(Value::BuiltinValidator {
                name: "list.UniqueItems()".to_string(),
                target: dummy,
            }))
        }
        ("list", "MatchN") => {
            let dummy = arena.alloc(Value::Top);
            Ok(arena.alloc(Value::BuiltinValidator {
                name: "list.MatchN".to_string(),
                target: dummy,
            }))
        }
        ("list", "Contains") => {
            if args.len() >= 2
                && let Some(Value::List { elements, .. }) = arena.get(args[0]) {
                    let target_id = args[1];
                    let contains = elements.contains(&target_id);
                    return Ok(arena.bool(contains));
                }
            Err("list.Contains requires 1 list and 1 element argument".to_string())
        }
        ("list", "Range") => {
            if args.len() >= 3
                && let (Some(Value::Int(start)), Some(Value::Int(end)), Some(Value::Int(step))) =
                    (arena.get(args[0]), arena.get(args[1]), arena.get(args[2]))
                    && let (Some(s), Some(e), Some(st)) = (start.to_i64(), end.to_i64(), step.to_i64())
                        && st != 0 {
                            let mut elements = Vec::new();
                            let mut cur = s;
                            while (st > 0 && cur < e) || (st < 0 && cur > e) {
                                elements.push(arena.int(cur));
                                cur += st;
                            }
                            return Ok(arena.alloc(Value::List {
                                elements,
                                ellipsis: None,
                            }));
                        }
            Err("list.Range requires start, end, step integer arguments".to_string())
        }
        ("list", "Take") => {
            if args.len() >= 2
                && let (Some(Value::List { elements, ellipsis }), Some(Value::Int(n_val))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    let n = n_val.to_usize().unwrap_or(0).min(elements.len());
                    let taken = elements[..n].to_vec();
                    let el = *ellipsis;
                    return Ok(arena.alloc(Value::List {
                        elements: taken,
                        ellipsis: el,
                    }));
                }
            Err("list.Take requires 1 list and 1 count argument".to_string())
        }
        ("list", "Drop") => {
            if args.len() >= 2
                && let (Some(Value::List { elements, ellipsis }), Some(Value::Int(n_val))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    let n = n_val.to_usize().unwrap_or(0).min(elements.len());
                    let dropped = elements[n..].to_vec();
                    let el = *ellipsis;
                    return Ok(arena.alloc(Value::List {
                        elements: dropped,
                        ellipsis: el,
                    }));
                }
            Err("list.Drop requires 1 list and 1 count argument".to_string())
        }
        ("list", "Sort") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, ellipsis }) = arena.get(arg0) {
                    let mut elems = elements.clone();
                    let el = *ellipsis;
                    elems.sort_by(|&a_id, &b_id| {
                        match (arena.get(a_id), arena.get(b_id)) {
                            (Some(Value::Int(a)), Some(Value::Int(b))) => a.cmp(b),
                            (Some(Value::Float(a)), Some(Value::Float(b))) => {
                                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                            }
                            (Some(Value::String(a)), Some(Value::String(b))) => a.cmp(b),
                            _ => std::cmp::Ordering::Equal,
                        }
                    });
                    return Ok(arena.alloc(Value::List {
                        elements: elems,
                        ellipsis: el,
                    }));
                }
            Err("list.Sort requires 1 list argument".to_string())
        }
        ("list", "SortStrings") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, ellipsis }) = arena.get(arg0) {
                    let mut elems = elements.clone();
                    let el = *ellipsis;
                    elems.sort_by(|&a_id, &b_id| {
                        match (arena.get(a_id), arena.get(b_id)) {
                            (Some(Value::String(a)), Some(Value::String(b))) => a.cmp(b),
                            _ => std::cmp::Ordering::Equal,
                        }
                    });
                    return Ok(arena.alloc(Value::List {
                        elements: elems,
                        ellipsis: el,
                    }));
                }
            Err("list.SortStrings requires 1 list of strings argument".to_string())
        }
        ("list", "IsSorted") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, .. }) = arena.get(arg0) {
                    let elems = elements.clone();
                    let sorted = elems.windows(2).all(|pair| {
                        match (arena.get(pair[0]), arena.get(pair[1])) {
                            (Some(Value::Int(a)), Some(Value::Int(b))) => a <= b,
                            (Some(Value::Float(a)), Some(Value::Float(b))) => a <= b,
                            (Some(Value::String(a)), Some(Value::String(b))) => a <= b,
                            _ => true,
                        }
                    });
                    return Ok(arena.bool(sorted));
                }
            Err("list.IsSorted requires 1 list argument".to_string())
        }
        ("list", "IsSortedStrings") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, .. }) = arena.get(arg0) {
                    let elems = elements.clone();
                    let sorted = elems.windows(2).all(|pair| {
                        match (arena.get(pair[0]), arena.get(pair[1])) {
                            (Some(Value::String(a)), Some(Value::String(b))) => a <= b,
                            _ => true,
                        }
                    });
                    return Ok(arena.bool(sorted));
                }
            Err("list.IsSortedStrings requires 1 list of strings argument".to_string())
        }
        ("list", "Reverse") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, ellipsis }) = arena.get(arg0) {
                    let mut elems = elements.clone();
                    let el = *ellipsis;
                    elems.reverse();
                    return Ok(arena.alloc(Value::List {
                        elements: elems,
                        ellipsis: el,
                    }));
                }
            Err("list.Reverse requires 1 list argument".to_string())
        }
        ("list", "Compact") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, ellipsis }) = arena.get(arg0) {
                    let elems = elements.clone();
                    let el = *ellipsis;
                    let compacted: Vec<ValueId> = elems.into_iter().filter(|&e| {
                        !matches!(arena.get(e), Some(Value::Null) | Some(Value::Bottom(_)) | None)
                    }).collect();
                    return Ok(arena.alloc(Value::List {
                        elements: compacted,
                        ellipsis: el,
                    }));
                }
            Err("list.Compact requires 1 list argument".to_string())
        }
        ("list", "Chunk") => {
            if args.len() >= 2
                && let (Some(Value::List { elements, .. }), Some(Value::Int(n_val))) =
                    (arena.get(args[0]), arena.get(args[1]))
                && let Some(chunk_size) = n_val.to_usize()
                && chunk_size > 0 {
                    let elems = elements.clone();
                    let chunked_lists: Vec<ValueId> = elems.chunks(chunk_size).map(|chunk| {
                        arena.alloc(Value::List {
                            elements: chunk.to_vec(),
                            ellipsis: None,
                        })
                    }).collect();
                    return Ok(arena.alloc(Value::List {
                        elements: chunked_lists,
                        ellipsis: None,
                    }));
                }
            Err("list.Chunk requires (list, size > 0) arguments".to_string())
        }
        ("list", "Distinct") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, ellipsis }) = arena.get(arg0) {
                    let elems = elements.clone();
                    let el = *ellipsis;
                    let mut seen = HashSet::new();
                    let mut unique = Vec::new();
                    for &e in &elems {
                        let repr = match arena.get(e) {
                            Some(Value::Int(i)) => format!("int:{i}"),
                            Some(Value::Float(f)) => format!("float:{f}"),
                            Some(Value::String(s)) => format!("str:{s}"),
                            Some(Value::Bool(b)) => format!("bool:{b}"),
                            _ => format!("id:{:?}", e),
                        };
                        if seen.insert(repr) {
                            unique.push(e);
                        }
                    }
                    return Ok(arena.alloc(Value::List {
                        elements: unique,
                        ellipsis: el,
                    }));
                }
            Err("list.Distinct requires 1 list argument".to_string())
        }
        ("list", "Zip") => {
            if args.len() >= 2
                && let (Some(Value::List { elements: l1, .. }), Some(Value::List { elements: l2, .. })) =
                    (arena.get(args[0]), arena.get(args[1])) {
                    let l1_elems = l1.clone();
                    let l2_elems = l2.clone();
                    let pairs: Vec<ValueId> = l1_elems.into_iter().zip(l2_elems).map(|(a, b)| {
                        arena.alloc(Value::List {
                            elements: vec![a, b],
                            ellipsis: None,
                        })
                    }).collect();
                    return Ok(arena.alloc(Value::List {
                        elements: pairs,
                        ellipsis: None,
                    }));
                }
            Err("list.Zip requires 2 list arguments".to_string())
        }
        ("list", "Unzip") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, .. }) = arena.get(arg0) {
                    let elems = elements.clone();
                    let mut l1 = Vec::new();
                    let mut l2 = Vec::new();
                    for &pair_id in &elems {
                        if let Some(Value::List { elements: pair, .. }) = arena.get(pair_id)
                            && pair.len() >= 2 {
                                l1.push(pair[0]);
                                l2.push(pair[1]);
                            }
                    }
                    let l1_id = arena.alloc(Value::List { elements: l1, ellipsis: None });
                    let l2_id = arena.alloc(Value::List { elements: l2, ellipsis: None });
                    return Ok(arena.alloc(Value::List {
                        elements: vec![l1_id, l2_id],
                        ellipsis: None,
                    }));
                }
            Err("list.Unzip requires 1 list of pairs argument".to_string())
        }
        ("list", "FlattenN") => {
            if args.len() >= 2
                && let (Some(Value::List { elements, .. }), Some(Value::Int(d_val))) =
                    (arena.get(args[0]), arena.get(args[1]))
                {
                    let depth = d_val.to_usize().unwrap_or(1);
                    let flattened = flatten_list(arena, elements, depth);
                    return Ok(arena.alloc(Value::List {
                        elements: flattened,
                        ellipsis: None,
                        }));
                }
            Err("list.FlattenN requires 1 list and 1 depth integer argument".to_string())
        }
        ("list", "Flatten") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, .. }) = arena.get(arg0) {
                    let flattened = flatten_list(arena, elements, 100);
                    return Ok(arena.alloc(Value::List {
                        elements: flattened,
                        ellipsis: None,
                    }));
                }
            Err("list.Flatten requires 1 list argument".to_string())
        }
        ("list", "Concat") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, .. }) = arena.get(arg0) {
                    let mut concatenated = Vec::new();
                    for &sub_id in elements {
                        if let Some(Value::List { elements: sub_elems, .. }) = arena.get(sub_id) {
                            concatenated.extend_from_slice(sub_elems);
                        } else {
                            concatenated.push(sub_id);
                        }
                    }
                    return Ok(arena.alloc(Value::List {
                        elements: concatenated,
                        ellipsis: None,
                    }));
                }
            Err("list.Concat requires 1 list of lists argument".to_string())
        }
        ("list", "Repeat") => {
            if args.len() >= 2
                && let Some(Value::Int(count_val)) = arena.get(args[1])
                && let Some(count) = count_val.to_usize() {
                    let elem = args[0];
                    let repeated = vec![elem; count];
                    return Ok(arena.alloc(Value::List {
                        elements: repeated,
                        ellipsis: None,
                    }));
                }
            Err("list.Repeat requires (elem, count) arguments".to_string())
        }
        ("list", "Slice") => {
            if args.len() >= 3
                && let (Some(Value::List { elements, .. }), Some(Value::Int(low_val)), Some(Value::Int(high_val))) =
                    (arena.get(args[0]), arena.get(args[1]), arena.get(args[2]))
                {
                    let low = low_val.to_usize().unwrap_or(0).min(elements.len());
                    let high = high_val.to_usize().unwrap_or(elements.len()).min(elements.len());
                    let sliced = if low <= high { elements[low..high].to_vec() } else { Vec::new() };
                    return Ok(arena.alloc(Value::List {
                        elements: sliced,
                        ellipsis: None,
                    }));
                }
            Err("list.Slice requires (list, low, high) arguments".to_string())
        }
        ("list", "Sum") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, .. }) = arena.get(arg0) {
                    let mut sum_int = num_bigint::BigInt::from(0);
                    let mut is_float = false;
                    let mut sum_float = 0.0;
                    for &elem in elements {
                        match arena.get(elem) {
                            Some(Value::Int(i)) => {
                                sum_int += i;
                                sum_float += i.to_f64().unwrap_or(0.0);
                            }
                            Some(Value::Float(f)) => {
                                is_float = true;
                                sum_float += f;
                            }
                            _ => return Err("list.Sum requires a list of numbers".to_string()),
                        }
                    }
                    if is_float {
                        return Ok(arena.float(sum_float));
                    } else {
                        return Ok(arena.int(sum_int));
                    }
                }
            Err("list.Sum requires 1 list argument".to_string())
        }
        ("list", "Product") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, .. }) = arena.get(arg0) {
                    let mut prod_int = num_bigint::BigInt::from(1);
                    let mut is_float = false;
                    let mut prod_float = 1.0;
                    for &elem in elements {
                        match arena.get(elem) {
                            Some(Value::Int(i)) => {
                                prod_int *= i;
                                prod_float *= i.to_f64().unwrap_or(1.0);
                            }
                            Some(Value::Float(f)) => {
                                is_float = true;
                                prod_float *= f;
                            }
                            _ => return Err("list.Product requires a list of numbers".to_string()),
                        }
                    }
                    if is_float {
                        return Ok(arena.float(prod_float));
                    } else {
                        return Ok(arena.int(prod_int));
                    }
                }
            Err("list.Product requires 1 list argument".to_string())
        }
        ("list", "Avg") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, .. }) = arena.get(arg0) {
                    if elements.is_empty() {
                        return Ok(arena.float(0.0));
                    }
                    let mut sum_float = 0.0;
                    for &elem in elements {
                        match arena.get(elem) {
                            Some(Value::Int(i)) => sum_float += i.to_f64().unwrap_or(0.0),
                            Some(Value::Float(f)) => sum_float += f,
                            _ => return Err("list.Avg requires a list of numbers".to_string()),
                        }
                    }
                    return Ok(arena.float(sum_float / elements.len() as f64));
                }
            Err("list.Avg requires 1 list argument".to_string())
        }
        ("list", "Min") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, .. }) = arena.get(arg0) {
                    if elements.is_empty() {
                        return Ok(arena.bottom("list.Min: empty list"));
                    }
                    let mut min_val = elements[0];
                    for &elem in &elements[1..] {
                        match (arena.get(min_val), arena.get(elem)) {
                            (Some(Value::Int(a)), Some(Value::Int(b))) if b < a => min_val = elem,
                            (Some(Value::Float(a)), Some(Value::Float(b))) if b < a => min_val = elem,
                            _ => {}
                        }
                    }
                    return Ok(min_val);
                }
            Err("list.Min requires 1 list argument".to_string())
        }
        ("list", "Max") => {
            if let Some(&arg0) = args.first()
                && let Some(Value::List { elements, .. }) = arena.get(arg0) {
                    if elements.is_empty() {
                        return Ok(arena.bottom("list.Max: empty list"));
                    }
                    let mut max_val = elements[0];
                    for &elem in &elements[1..] {
                        match (arena.get(max_val), arena.get(elem)) {
                            (Some(Value::Int(a)), Some(Value::Int(b))) if b > a => max_val = elem,
                            (Some(Value::Float(a)), Some(Value::Float(b))) if b > a => max_val = elem,
                            _ => {}
                        }
                    }
                    return Ok(max_val);
                }
            Err("list.Max requires 1 list argument".to_string())
        }
        _ => Err(format!("unknown list function: list.{func_name}")),
    }
}

fn flatten_list(arena: &ValueArena, elements: &[ValueId], depth: usize) -> Vec<ValueId> {
    let mut out = Vec::new();
    for &elem in elements {
        if depth > 0
            && let Some(Value::List { elements: inner, .. }) = arena.get(elem) {
                out.extend(flatten_list(arena, inner, depth - 1));
                continue;
            }
        out.push(elem);
    }
    out
}
