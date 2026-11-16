//! Signal values and how to turn them into text.

/// A single signal value, independent of the file format it came from.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A bit vector, most significant bit first.
    ///
    /// Each character is one of the nine `std_logic` states:
    /// `0 1 x z h u w l -` (lowercase, as produced by wellen).
    Bits(String),
    Real(f64),
    /// Text, including the name of an enumeration literal (e.g. `IDLE`).
    Text(String),
    /// A VHDL/Verilog event without a value.
    Event,
}

/// How to display a bit vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Radix {
    Binary,
    #[default]
    Hex,
    Unsigned,
    Signed,
    Ascii,
}

impl Radix {
    pub const ALL: [Radix; 5] = [
        Radix::Binary,
        Radix::Hex,
        Radix::Unsigned,
        Radix::Signed,
        Radix::Ascii,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Radix::Binary => "Binary",
            Radix::Hex => "Hexadecimal",
            Radix::Unsigned => "Unsigned decimal",
            Radix::Signed => "Signed decimal",
            Radix::Ascii => "ASCII",
        }
    }
}

impl Value {
    /// Format the value for display. `radix` only matters for bit vectors.
    pub fn format(&self, radix: Radix) -> String {
        match self {
            Value::Bits(bits) => format_bits(bits, radix),
            Value::Real(r) => format!("{r}"),
            Value::Text(s) => s.clone(),
            Value::Event => "event".to_string(),
        }
    }

    /// For a 1-bit value, its single state character (`'0'`, `'1'`, `'x'`...).
    pub fn bit(&self) -> Option<char> {
        match self {
            Value::Bits(b) if b.len() == 1 => b.chars().next(),
            _ => None,
        }
    }
}

/// Converts a weak or strong 0/1 to a number; anything else (x, z, u, ...) is unknown.
///
/// `h`/`l` are the "weak" high/low of `std_logic`; numerically they are 1/0.
fn bit_value(c: char) -> Option<u8> {
    match c {
        '0' | 'l' => Some(0),
        '1' | 'h' => Some(1),
        _ => None,
    }
}

/// Format a bit string (MSB first) in the given radix.
pub fn format_bits(bits: &str, radix: Radix) -> String {
    match radix {
        Radix::Binary => bits.to_string(),
        Radix::Hex => format_hex(bits),
        Radix::Unsigned => match to_u128(bits) {
            Some(v) => v.to_string(),
            None => unknown_summary(bits),
        },
        Radix::Signed => match to_u128(bits) {
            Some(v) => to_signed(v, bits.len()).to_string(),
            None => unknown_summary(bits),
        },
        Radix::Ascii => format_ascii(bits),
    }
}

/// Group the bits by 4 from the LSB side and print one hex digit per group.
///
/// A group that contains non-0/1 states prints that state if the whole group has it
/// (e.g. `zzzz` -> `z`), and `X` for a mix.
fn format_hex(bits: &str) -> String {
    let chars: Vec<char> = bits.chars().collect();
    // `rchunks` walks from the end of the slice: exactly the LSB-first grouping we need.
    let mut digits: Vec<char> = chars
        .rchunks(4)
        .map(|group| {
            let mut value = 0u32;
            for &c in group {
                match bit_value(c) {
                    Some(b) => value = value * 2 + u32::from(b),
                    None => {
                        return if group.iter().all(|&g| g == c) {
                            c
                        } else {
                            'X'
                        };
                    }
                }
            }
            char::from_digit(value, 16).unwrap().to_ascii_uppercase()
        })
        .collect();
    digits.reverse();
    digits.into_iter().collect()
}

/// Interpret the bits as an unsigned number. `None` if any bit is unknown or the
/// vector is too wide for 128 bits.
fn to_u128(bits: &str) -> Option<u128> {
    if bits.len() > 128 {
        return None;
    }
    bits.chars()
        .try_fold(0u128, |acc, c| Some((acc << 1) | u128::from(bit_value(c)?)))
}

/// Two's complement: if the top bit is set, subtract 2^width.
fn to_signed(value: u128, width: usize) -> i128 {
    if width == 0 {
        return 0;
    }
    if width >= 128 {
        return value as i128;
    }
    let sign_bit = 1u128 << (width - 1);
    if value & sign_bit != 0 {
        value as i128 - (1i128 << width)
    } else {
        value as i128
    }
}

/// What to show in a decimal radix when the value contains unknown bits.
fn unknown_summary(bits: &str) -> String {
    if bits.len() > 128 {
        return format_hex(bits);
    }
    let first_unknown = bits.chars().find(|&c| bit_value(c).is_none());
    match first_unknown {
        Some(c) if bits.chars().all(|b| b == c) => c.to_string(),
        _ => "X".to_string(),
    }
}

/// Every 8 bits (from the LSB side) become one character; non-printable bytes become `.`.
fn format_ascii(bits: &str) -> String {
    let chars: Vec<char> = bits.chars().collect();
    let mut out: Vec<char> = chars
        .rchunks(8)
        .map(|byte| {
            let value = byte
                .iter()
                .try_fold(0u8, |acc, &c| Some((acc << 1) | bit_value(c)?));
            match value {
                Some(v) if (0x20..0x7f).contains(&v) => v as char,
                _ => '.',
            }
        })
        .collect();
    out.reverse();
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex() {
        assert_eq!(format_bits("00001010", Radix::Hex), "0A");
        assert_eq!(format_bits("111111111", Radix::Hex), "1FF");
        assert_eq!(format_bits("zzzz0001", Radix::Hex), "z1");
        assert_eq!(format_bits("0x010000", Radix::Hex), "X0");
        assert_eq!(format_bits("hl", Radix::Hex), "2");
    }

    #[test]
    fn decimal() {
        assert_eq!(format_bits("1111", Radix::Unsigned), "15");
        assert_eq!(format_bits("1111", Radix::Signed), "-1");
        assert_eq!(format_bits("0111", Radix::Signed), "7");
        assert_eq!(format_bits("1000", Radix::Signed), "-8");
        assert_eq!(format_bits("uuuu", Radix::Unsigned), "u");
        assert_eq!(format_bits("01x1", Radix::Signed), "X");
    }

    #[test]
    fn wide_values() {
        let bits = "1".repeat(128);
        assert_eq!(format_bits(&bits, Radix::Unsigned), u128::MAX.to_string());
        assert_eq!(format_bits(&bits, Radix::Signed), "-1");
        // Too wide for a number: falls back to hex.
        let bits = "1".repeat(132);
        assert_eq!(format_bits(&bits, Radix::Unsigned), "F".repeat(33));
    }

    #[test]
    fn ascii() {
        // "Hi" = 0x48 0x69
        assert_eq!(format_bits("0100100001101001", Radix::Ascii), "Hi");
        assert_eq!(format_bits("00000001", Radix::Ascii), ".");
    }

    #[test]
    fn other_values() {
        assert_eq!(Value::Text("IDLE".into()).format(Radix::Hex), "IDLE");
        assert_eq!(Value::Real(1.5).format(Radix::Hex), "1.5");
        assert_eq!(Value::Bits("1".into()).bit(), Some('1'));
        assert_eq!(Value::Bits("10".into()).bit(), None);
    }
}
