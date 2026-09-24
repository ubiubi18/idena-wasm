//! Differential test: the idena-wasm `ProtoArgs` decoder must accept exactly
//! the inputs that rust-protobuf 2.27.1 accepts, and decode them identically.
//!
//! Run longer with:
//!   PROTO_ARGS_FUZZ_ITERATIONS=10000000 cargo test --release
//! Reproduce a failure with the printed seed:
//!   PROTO_ARGS_FUZZ_SEED=<seed> PROTO_ARGS_FUZZ_ITERATIONS=<n> cargo test

use proto_args_differential::{decode_current, decode_legacy, decode_protobuf3};

const DEFAULT_ITERATIONS: u64 = 300_000;
const DEFAULT_SEED: u64 = 0x1de7_a5ee_d000_0001;

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn assert_same(data: &[u8], context: &str) {
    let legacy = decode_legacy(data);
    let current = decode_current(data);
    assert_eq!(
        current,
        legacy,
        "decoder diverges from rust-protobuf 2.27.1 ({}) on input {}",
        context,
        hex(data)
    );
}

fn env_u64(name: &str, default: u64) -> u64 {
    match std::env::var(name) {
        Ok(value) => {
            let value = value.trim();
            match value.strip_prefix("0x") {
                Some(hex) => u64::from_str_radix(hex, 16),
                None => value.parse(),
            }
            .unwrap_or_else(|_| panic!("invalid {}", name))
        }
        Err(_) => default,
    }
}

/// SplitMix64: small, deterministic, good enough for input generation.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }
}

fn push_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// Encodes a varint, sometimes non-canonically: padded, over-long, or with
/// bits beyond 64 set in the tenth byte.
fn push_varint_fuzzy(rng: &mut Rng, out: &mut Vec<u8>, value: u64) {
    match rng.below(20) {
        0 => {
            // Padded with continuation bytes, up to 10 bytes total.
            let mut encoded = Vec::new();
            push_varint(&mut encoded, value);
            let target = encoded.len() + rng.below((10 - encoded.len() as u64) + 1) as usize;
            while encoded.len() < target {
                let last = encoded.len() - 1;
                encoded[last] |= 0x80;
                encoded.push(0);
            }
            out.extend(encoded);
        }
        1 => {
            // Ten bytes with garbage above bit 64.
            let mut v = value;
            for _ in 0..9 {
                out.push((v & 0x7f) as u8 | 0x80);
                v >>= 7;
            }
            out.push((v as u8 & 0x01) | (rng.next() as u8 & 0x7e));
        }
        2 => {
            // Eleven bytes: too long.
            out.extend(std::iter::repeat(0x80).take(10));
            out.push(0x01);
        }
        _ => push_varint(out, value),
    }
}

fn interesting_u64(rng: &mut Rng) -> u64 {
    const VALUES: [u64; 12] = [
        0,
        1,
        2,
        127,
        128,
        0xffff_ffff,
        1 << 32,
        (1 << 32) + 1,
        (1 << 32) + 5,
        u64::MAX,
        1 << 63,
        i64::MAX as u64,
    ];
    if rng.chance(60) {
        VALUES[rng.below(VALUES.len() as u64) as usize]
    } else {
        rng.next() >> rng.below(64)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Level {
    ProtoArgs,
    Argument,
    Group,
}

fn expected_wire_type(level: Level, field: u32) -> Option<u32> {
    match (level, field) {
        (Level::ProtoArgs, 1) => Some(2),
        (Level::Argument, 1) => Some(2),
        (Level::Argument, 2) => Some(0),
        _ => None,
    }
}

fn pick_field(rng: &mut Rng, level: Level) -> u32 {
    match rng.below(20) {
        0..=7 => 1,
        8..=11 => 2,
        12 => 0,
        13 => (1 << 29) - 1,
        14 => rng.next() as u32 >> 3,
        _ => 3 + rng.below(if level == Level::Group { 5 } else { 30 }) as u32,
    }
}

fn pick_wire_type(rng: &mut Rng, level: Level, field: u32) -> u32 {
    match expected_wire_type(level, field) {
        Some(expected) if rng.chance(70) => expected,
        _ => rng.below(8) as u32,
    }
}

fn push_tag(rng: &mut Rng, out: &mut Vec<u8>, field: u32, wire_type: u32) {
    let mut tag = (u64::from(field) << 3) | u64::from(wire_type);
    if rng.chance(3) {
        // Bits above 32 are dropped by rust-protobuf 2 tag parsing.
        tag |= (1 + rng.below(0xff)) << 32;
    }
    push_varint_fuzzy(rng, out, tag);
}

/// Length prefix for `content`: usually correct, sometimes off or with high
/// bits that only a 32-bit truncating reader ignores.
fn push_length(rng: &mut Rng, out: &mut Vec<u8>, len: usize) {
    let len = len as u64;
    let value = match rng.below(25) {
        0 => len + 1 + rng.below(4),
        1 => len.saturating_sub(1 + rng.below(4)),
        2 => len | (1 << 32),
        3 => len | (rng.next() & !0xffff_ffff),
        4 => u64::MAX - rng.below(4),
        _ => len,
    };
    push_varint_fuzzy(rng, out, value);
}

fn gen_payload(
    rng: &mut Rng,
    out: &mut Vec<u8>,
    level: Level,
    field: u32,
    wire_type: u32,
    depth: u32,
) {
    match wire_type {
        0 => {
            let value = interesting_u64(rng);
            push_varint_fuzzy(rng, out, value);
        }
        1 | 5 => {
            let width = if wire_type == 1 { 8 } else { 4 };
            let len = if rng.chance(90) {
                width
            } else {
                rng.below(width as u64) as usize
            };
            out.extend(rng.bytes(len));
        }
        2 => {
            let content = if level == Level::ProtoArgs && field == 1 && depth < 3 && rng.chance(80)
            {
                gen_fields(rng, Level::Argument, depth + 1)
            } else if rng.chance(15) && depth < 3 {
                gen_fields(rng, Level::ProtoArgs, depth + 1)
            } else {
                let len = rng.below(12) as usize;
                rng.bytes(len)
            };
            push_length(rng, out, content.len());
            out.extend(content);
        }
        3 => {
            if depth < 3 {
                out.extend(gen_fields(rng, Level::Group, depth + 1));
            }
            if rng.chance(85) {
                let end_field = if rng.chance(50) {
                    field
                } else {
                    pick_field(rng, Level::Group)
                };
                push_tag(rng, out, end_field, 4);
            }
        }
        4 => {}
        _ => {
            let len = rng.below(4) as usize;
            out.extend(rng.bytes(len));
        }
    }
}

fn gen_fields(rng: &mut Rng, level: Level, depth: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let count = rng.below(6);
    for _ in 0..count {
        let field = pick_field(rng, level);
        let wire_type = pick_wire_type(rng, level, field);
        push_tag(rng, &mut out, field, wire_type);
        gen_payload(rng, &mut out, level, field, wire_type, depth);
    }
    out
}

fn mutate(rng: &mut Rng, data: &mut Vec<u8>) {
    let rounds = 1 + rng.below(3);
    for _ in 0..rounds {
        match rng.below(4) {
            0 if !data.is_empty() => {
                let i = rng.below(data.len() as u64) as usize;
                data[i] ^= 1 << rng.below(8);
            }
            1 if !data.is_empty() => {
                let i = rng.below(data.len() as u64) as usize;
                data[i] = rng.next() as u8;
            }
            2 if !data.is_empty() => {
                let len = rng.below(data.len() as u64) as usize;
                data.truncate(len);
            }
            _ => {
                let i = rng.below(data.len() as u64 + 1) as usize;
                data.insert(i, rng.next() as u8);
            }
        }
    }
}

fn gen_input(rng: &mut Rng) -> Vec<u8> {
    if rng.chance(5) {
        let len = rng.below(24) as usize;
        return rng.bytes(len);
    }
    let mut data = gen_fields(rng, Level::ProtoArgs, 0);
    if rng.chance(15) {
        mutate(rng, &mut data);
    }
    data
}

#[test]
fn differential_fuzz_matches_legacy_decoder() {
    let seed = env_u64("PROTO_ARGS_FUZZ_SEED", DEFAULT_SEED);
    let iterations = env_u64("PROTO_ARGS_FUZZ_ITERATIONS", DEFAULT_ITERATIONS);
    let mut rng = Rng(seed);

    let mut accepted = 0u64;
    let mut protobuf3_divergences = 0u64;
    for i in 0..iterations {
        let data = gen_input(&mut rng);
        let legacy = decode_legacy(&data);
        assert_eq!(
            decode_current(&data),
            legacy,
            "decoder diverges from rust-protobuf 2.27.1 on input {} (seed {:#x}, iteration {})",
            hex(&data),
            seed,
            i
        );
        if legacy.is_some() {
            accepted += 1;
        }
        if decode_protobuf3(&data) != legacy {
            protobuf3_divergences += 1;
        }
    }

    println!(
        "seed {:#x}: {} inputs, {} accepted by rust-protobuf 2, {} where rust-protobuf 3 diverges",
        seed, iterations, accepted, protobuf3_divergences
    );
    // Guard against a generator that only produces trivially invalid input.
    assert!(accepted * 10 >= iterations, "generator coverage too low");
}

#[test]
fn exhaustive_short_inputs_match_legacy_decoder() {
    assert_same(&[], "empty");
    for a in 0..=255u8 {
        assert_same(&[a], "1 byte");
        for b in 0..=255u8 {
            assert_same(&[a, b], "2 bytes");
        }
    }

    // Three and four bytes over tag, varint and length boundary values.
    const ALPHABET: [u8; 20] = [
        0x00, 0x01, 0x02, 0x03, 0x05, 0x08, 0x0a, 0x0b, 0x0c, 0x10, 0x12, 0x13, 0x14, 0x1b, 0x1c,
        0x7f, 0x80, 0x81, 0xf0, 0xff,
    ];
    for &a in &ALPHABET {
        for &b in &ALPHABET {
            for &c in &ALPHABET {
                assert_same(&[a, b, c], "3 bytes");
                for &d in &ALPHABET {
                    assert_same(&[a, b, c, d], "4 bytes");
                }
            }
        }
    }
}

#[test]
fn known_protobuf3_divergences_are_fixed() {
    for payload in [
        "0801",             // ProtoArgs.args as varint
        "0a020805",         // Argument.value as varint
        "0a03120100",       // Argument.is_nil as length-delimited
        "0a030a01410801",   // valid argument followed by args as varint
        "0b0c",             // ProtoArgs.args as group
        "0a06108080808010", // Argument.is_nil = 2^32
    ] {
        let data = unhex(payload);
        assert_ne!(
            decode_protobuf3(&data),
            decode_legacy(&data),
            "expected rust-protobuf 3 to diverge on {}",
            payload
        );
        assert_same(&data, payload);
    }
}

#[test]
fn legacy_quirks_found_by_fuzzing_are_preserved() {
    // rust-protobuf 2 bounds a nested message by a logical limit only, so an
    // argument that declares more bytes than remain parses to end of input.
    for (payload, expected) in [
        ("0a01", vec![(vec![], false)]),
        (
            "0a87808080100a054d6b3a997e",
            vec![(unhex("4d6b3a997e"), false)],
        ),
    ] {
        let data = unhex(payload);
        assert_eq!(decode_legacy(&data), Some(expected.clone()), "{}", payload);
        assert_eq!(decode_current(&data), Some(expected), "{}", payload);
    }
}

#[test]
fn valid_arguments_round_trip() {
    let mut data = Vec::new();
    for (value, is_nil) in [
        (&b"A"[..], false),
        (&b""[..], true),
        (&[0u8; 300][..], false),
    ] {
        let mut arg = Vec::new();
        arg.push(0x0a);
        push_varint(&mut arg, value.len() as u64);
        arg.extend_from_slice(value);
        if is_nil {
            arg.extend([0x10, 0x01]);
        }
        data.push(0x0a);
        push_varint(&mut data, arg.len() as u64);
        data.extend(arg);
    }

    let decoded = decode_current(&data).unwrap();
    assert_eq!(decoded, decode_legacy(&data).unwrap());
    assert_eq!(decoded.len(), 3);
    assert_eq!(decoded[2].0.len(), 300);
}
