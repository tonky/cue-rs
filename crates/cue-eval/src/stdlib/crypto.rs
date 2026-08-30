use crate::value::{Value, ValueArena, ValueId};

pub fn call_crypto(
    arena: &mut ValueArena,
    pkg: &str,
    func_name: &str,
    args: &[ValueId],
) -> Result<ValueId, String> {
    match (pkg, func_name) {
// --- crypto/sha256 package ---
        ("sha256" | "crypto/sha256", "Sum" | "Sum256") => {
            if let Some(&arg0) = args.first() {
                let bytes = match arena.get(arg0) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                if let Some(b) = bytes {
                    return Ok(arena.alloc(Value::Bytes(sha256_bytes(&b))));
                }
            }
            Err("sha256.Sum requires 1 string or bytes argument".to_string())
        }
        ("sha256" | "crypto/sha256", "Sum224") => {
            if let Some(&arg0) = args.first() {
                let bytes = match arena.get(arg0) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                if let Some(b) = bytes {
                    let mut full = sha256_bytes(&b);
                    full.truncate(28);
                    return Ok(arena.alloc(Value::Bytes(full)));
                }
            }
            Err("sha256.Sum224 requires 1 string or bytes argument".to_string())
        }

        // --- crypto/md5 package ---
        ("md5" | "crypto/md5", "Sum") => {
            if let Some(&arg0) = args.first() {
                let bytes = match arena.get(arg0) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                if let Some(b) = bytes {
                    return Ok(arena.string(md5_digest(&b)));
                }
            }
            Err("md5.Sum requires 1 string or bytes argument".to_string())
        }

        // --- crypto/sha1 package ---
        ("sha1" | "crypto/sha1", "Sum") => {
            if let Some(&arg0) = args.first() {
                let bytes = match arena.get(arg0) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                if let Some(b) = bytes {
                    return Ok(arena.string(sha1_digest(&b)));
                }
            }
            Err("sha1.Sum requires 1 string or bytes argument".to_string())
        }

        // --- crypto/sha512 package ---
        ("sha512" | "crypto/sha512", "Sum" | "Sum512") => {
            if let Some(&arg0) = args.first() {
                let bytes = match arena.get(arg0) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                if let Some(b) = bytes {
                    return Ok(arena.string(sha512_digest(&b)));
                }
            }
            Err("sha512.Sum requires 1 string or bytes argument".to_string())
        }

        // --- crypto/hmac package ---
        ("hmac" | "crypto/hmac", "SHA1") if args.is_empty() => Ok(arena.string("SHA1")),
        ("hmac" | "crypto/hmac", "SHA256") if args.is_empty() => Ok(arena.string("SHA256")),
        ("hmac" | "crypto/hmac", "SHA224") if args.is_empty() => Ok(arena.string("SHA224")),
        ("hmac" | "crypto/hmac", "SHA384") if args.is_empty() => Ok(arena.string("SHA384")),
        ("hmac" | "crypto/hmac", "SHA512") if args.is_empty() => Ok(arena.string("SHA512")),
        ("hmac" | "crypto/hmac", "MD5") if args.is_empty() => Ok(arena.string("MD5")),
        ("hmac" | "crypto/hmac", "Sign") => {
            if args.len() >= 3 {
                let hash_name = match arena.get(args[0]) {
                    Some(Value::String(s)) => s.as_str(),
                    _ => "",
                };
                let key = match arena.get(args[1]) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                let msg = match arena.get(args[2]) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                if let (Some(k), Some(m)) = (key, msg) {
                    let sig = match hash_name {
                        "SHA1" => hmac_bytes(&k, &m, sha1_bytes, 64),
                        "MD5" => hmac_bytes(&k, &m, md5_bytes, 64),
                        "SHA224" => {
                            let mut full = hmac_bytes(&k, &m, sha256_bytes, 64);
                            full.truncate(28);
                            full
                        }
                        "SHA384" => {
                            let mut full = hmac_bytes(&k, &m, sha512_bytes, 128);
                            full.truncate(48);
                            full
                        }
                        "SHA512" => hmac_bytes(&k, &m, sha512_bytes, 128),
                        _ => hmac_bytes(&k, &m, sha256_bytes, 64),
                    };
                    return Ok(arena.alloc(Value::Bytes(sig)));
                }
            }
            Err("hmac.Sign requires (hash, key, message) arguments".to_string())
        }
        ("hmac" | "crypto/hmac", "SHA256") => {
            if args.len() >= 2 {
                let a0 = match arena.get(args[0]) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                let a1 = match arena.get(args[1]) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                if let (Some(msg), Some(key)) = (a0, a1) {
                    return Ok(arena.string(hmac(&key, &msg, sha256_bytes, 64)));
                }
            }
            Err("hmac.SHA256 requires (message, key) arguments".to_string())
        }
        ("hmac" | "crypto/hmac", "SHA512") => {
            if args.len() >= 2 {
                let a0 = match arena.get(args[0]) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                let a1 = match arena.get(args[1]) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                if let (Some(msg), Some(key)) = (a0, a1) {
                    return Ok(arena.string(hmac(&key, &msg, sha512_bytes, 128)));
                }
            }
            Err("hmac.SHA512 requires (message, key) arguments".to_string())
        }
        ("hmac" | "crypto/hmac", "MD5") => {
            if args.len() >= 2 {
                let a0 = match arena.get(args[0]) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                let a1 = match arena.get(args[1]) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                if let (Some(msg), Some(key)) = (a0, a1) {
                    return Ok(arena.string(hmac(&key, &msg, md5_bytes, 64)));
                }
            }
            Err("hmac.MD5 requires (message, key) arguments".to_string())
        }
        ("hmac" | "crypto/hmac", "SHA1") => {
            if args.len() >= 2 {
                let a0 = match arena.get(args[0]) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                let a1 = match arena.get(args[1]) {
                    Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
                    Some(Value::Bytes(b)) => Some(b.clone()),
                    _ => None,
                };
                if let (Some(msg), Some(key)) = (a0, a1) {
                    return Ok(arena.string(hmac(&key, &msg, sha1_bytes, 64)));
                }
            }
            Err("hmac.SHA1 requires (message, key) arguments".to_string())
        }
        _ => Err(format!("unknown crypto function: {pkg}.{func_name}")),
    }
}

fn sha256_bytes(input: &[u8]) -> Vec<u8> {
    // Pure Rust SHA-256 implementation
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    let mut msg = input.to_vec();
    let bit_len = (input.len() as u64) * 8;
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut h_var = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h_var.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h_var = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(h_var);
    }

    let mut out = Vec::with_capacity(32);
    for val in h {
        out.extend_from_slice(&val.to_be_bytes());
    }
    out
}

fn hmac_bytes(
    key: &[u8],
    msg: &[u8],
    hash_fn: fn(&[u8]) -> Vec<u8>,
    block_size: usize,
) -> Vec<u8> {
    let mut k = if key.len() > block_size {
        hash_fn(key)
    } else {
        key.to_vec()
    };
    while k.len() < block_size {
        k.push(0);
    }

    let mut ipad = vec![0x36u8; block_size];
    let mut opad = vec![0x5cu8; block_size];

    for i in 0..block_size {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }

    let mut inner_input = ipad;
    inner_input.extend_from_slice(msg);
    let inner_hash = hash_fn(&inner_input);

    let mut outer_input = opad;
    outer_input.extend_from_slice(&inner_hash);
    hash_fn(&outer_input)
}

fn hmac(
    key: &[u8],
    msg: &[u8],
    hash_fn: fn(&[u8]) -> Vec<u8>,
    block_size: usize,
) -> String {
    let bytes = hmac_bytes(key, msg, hash_fn, block_size);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn md5_bytes(input: &[u8]) -> Vec<u8> {
    let mut a: u32 = 0x67452301;
    let mut b: u32 = 0xefcdab89;
    let mut c: u32 = 0x98badcfe;
    let mut d: u32 = 0x10325476;

    let s = [
        7, 12, 17, 22,  7, 12, 17, 22,  7, 12, 17, 22,  7, 12, 17, 22,
        5,  9, 14, 20,  5,  9, 14, 20,  5,  9, 14, 20,  5,  9, 14, 20,
        4, 11, 16, 23,  4, 11, 16, 23,  4, 11, 16, 23,  4, 11, 16, 23,
        6, 10, 15, 21,  6, 10, 15, 21,  6, 10, 15, 21,  6, 10, 15, 21,
    ];

    let k: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
        0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
        0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
        0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
        0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
        0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];

    let mut msg = input.to_vec();
    let bit_len = (input.len() as u64) * 8;
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());

    for chunk in msg.chunks(64) {
        let mut m = [0u32; 16];
        for i in 0..16 {
            m[i] = u32::from_le_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        let mut aa = a;
        let mut bb = b;
        let mut cc = c;
        let mut dd = d;

        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((bb & cc) | ((!bb) & dd), i),
                16..=31 => ((dd & bb) | ((!dd) & cc), (5 * i + 1) % 16),
                32..=47 => (bb ^ cc ^ dd, (3 * i + 5) % 16),
                _ => (cc ^ (bb | (!dd)), (7 * i) % 16),
            };
            let temp = dd;
            dd = cc;
            cc = bb;
            bb = bb.wrapping_add((aa.wrapping_add(f).wrapping_add(k[i]).wrapping_add(m[g])).rotate_left(s[i]));
            aa = temp;
        }

        a = a.wrapping_add(aa);
        b = b.wrapping_add(bb);
        c = c.wrapping_add(cc);
        d = d.wrapping_add(dd);
    }

    let mut out = Vec::with_capacity(16);
    for word in [a, b, c, d] {
        for byte in word.to_le_bytes() {
            out.push(byte);
        }
    }
    out
}

fn md5_digest(input: &[u8]) -> String {
    let bytes = md5_bytes(input);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha1_bytes(input: &[u8]) -> Vec<u8> {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let mut msg = input.to_vec();
    let bit_len = (input.len() as u64) * 8;
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for (i, &w_i) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w_i);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = Vec::with_capacity(20);
    for word in [h0, h1, h2, h3, h4] {
        out.extend_from_slice(&word.to_be_bytes());
    }
    out
}

fn sha1_digest(input: &[u8]) -> String {
    let bytes = sha1_bytes(input);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha512_bytes(input: &[u8]) -> Vec<u8> {
    let mut h = [
        0x6a09e667f3bcc908u64,
        0xbb67ae8584caa73bu64,
        0x3c6ef372fe94f82bu64,
        0xa54ff53a5f1d36f1u64,
        0x510e527fade682d1u64,
        0x9b05688c2b3e6c1fu64,
        0x1f83d9abfb41bd6bu64,
        0x5be0cd19137e2179u64,
    ];

    let k: [u64; 80] = [
        0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc,
        0x3956c25bf348b538, 0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118,
        0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
        0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694,
        0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65,
        0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
        0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4,
        0xc6e00bf33da88fc2, 0xd5a79147930aa725, 0x06ca6351e003826f, 0x142929670a0e6e70,
        0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
        0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
        0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30,
        0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
        0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8,
        0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3,
        0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
        0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b,
        0xca273eceea26619c, 0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178,
        0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
        0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c,
        0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
    ];

    let mut msg = input.to_vec();
    let bit_len = (input.len() as u128) * 8;
    msg.push(0x80);
    while (msg.len() % 128) != 112 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(128) {
        let mut w = [0u64; 80];
        for i in 0..16 {
            w[i] = u64::from_be_bytes(chunk[i * 8..i * 8 + 8].try_into().unwrap());
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for (i, &w_i) in w.iter().enumerate() {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w_i);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = Vec::with_capacity(64);
    for word in h {
        out.extend_from_slice(&word.to_be_bytes());
    }
    out
}

fn sha512_digest(input: &[u8]) -> String {
    let bytes = sha512_bytes(input);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
