use std::cell::Cell;
use std::collections::HashSet;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::Deserializer;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JsonValidationError {
    Invalid,
    DuplicateObjectKey,
}

pub(crate) fn validate_unique_json(input: &str) -> Result<(), JsonValidationError> {
    let duplicate_object_key = Cell::new(false);
    let mut deserializer = serde_json::Deserializer::from_str(input);
    if UniqueJsonValue(&duplicate_object_key)
        .deserialize(&mut deserializer)
        .is_err()
    {
        return Err(if duplicate_object_key.get() {
            JsonValidationError::DuplicateObjectKey
        } else {
            JsonValidationError::Invalid
        });
    }
    deserializer.end().map_err(|_| JsonValidationError::Invalid)
}

struct UniqueJsonValue<'a>(&'a Cell<bool>);

impl<'de> DeserializeSeed<'de> for UniqueJsonValue<'_> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(UniqueJsonVisitor(self.0))
    }
}

struct UniqueJsonVisitor<'a>(&'a Cell<bool>);

impl<'de> Visitor<'de> for UniqueJsonVisitor<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value without duplicate object keys")
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_string<E>(self, _value: String) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence
            .next_element_seed(UniqueJsonValue(self.0))?
            .is_some()
        {}
        Ok(())
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut keys = HashSet::new();
        while let Some(key) = object.next_key::<String>()? {
            if !keys.insert(key) {
                self.0.set(true);
                return Err(de::Error::custom("duplicate JSON object key"));
            }
            object.next_value_seed(UniqueJsonValue(self.0))?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{validate_unique_json, JsonValidationError};

    pub(crate) fn deterministic_json_fuzz_inputs() -> Vec<String> {
        const CASES: usize = 10_000;
        const TOKENS: &[u8] = b"{}[],:\"\\/0123456789.-+eEtruefalsn \t\r\n";
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut inputs = Vec::with_capacity(CASES);

        for case_number in 0..CASES {
            let input = match case_number % 4 {
                0 => {
                    let length = (next_fuzz_value(&mut state) % 256) as usize;
                    (0..length)
                        .map(|_| {
                            TOKENS[(next_fuzz_value(&mut state) as usize) % TOKENS.len()] as char
                        })
                        .collect()
                }
                1 => {
                    let depth = (next_fuzz_value(&mut state) % 192) as usize;
                    format!("{}null{}", "[".repeat(depth), "]".repeat(depth))
                }
                2 => {
                    let duplicate_key = if next_fuzz_value(&mut state) % 2 == 0 {
                        "nested"
                    } else {
                        "escaped\\\"key"
                    };
                    let nested_depth = (next_fuzz_value(&mut state) % 32) as usize;
                    format!(
                        "{{\"{duplicate_key}\":{},\"{duplicate_key}\":{}}}",
                        "[".repeat(nested_depth) + "0" + &"]".repeat(nested_depth),
                        case_number
                    )
                }
                _ => {
                    let mut encoded_text = String::new();
                    let length = (next_fuzz_value(&mut state) % 64) as usize;
                    for _ in 0..length {
                        match next_fuzz_value(&mut state) % 6 {
                            0 => encoded_text.push('a'),
                            1 => encoded_text.push_str("\\\""),
                            2 => encoded_text.push_str("\\\\"),
                            3 => encoded_text.push_str("\\n"),
                            4 => encoded_text.push_str("\\u0061"),
                            _ => encoded_text.push('é'),
                        }
                    }
                    format!("{{\"case\":{case_number},\"value\":\"{encoded_text}\"}}")
                }
            };
            inputs.push(input);
        }

        inputs
    }

    fn next_fuzz_value(state: &mut u64) -> u64 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *state
    }

    #[test]
    fn rejects_duplicate_keys_recursively_and_accepts_unique_json() {
        assert_eq!(
            validate_unique_json(r#"{"outer":{"key":1,"key":2}}"#),
            Err(JsonValidationError::DuplicateObjectKey)
        );
        assert_eq!(
            validate_unique_json(r#"{"outer":[{"key":1},{"key":2}]}"#),
            Ok(())
        );
    }

    #[test]
    fn rejects_invalid_json() {
        assert_eq!(
            validate_unique_json(r#"{"key":}"#),
            Err(JsonValidationError::Invalid)
        );
    }

    #[test]
    fn deterministic_json_parser_fuzzes_ten_thousand_cases_without_panicking() {
        let inputs = deterministic_json_fuzz_inputs();
        assert_eq!(inputs.len(), 10_000);

        for (case_number, input) in inputs.iter().enumerate() {
            let result = std::panic::catch_unwind(|| validate_unique_json(input));
            assert!(
                result.is_ok(),
                "JSON parser panicked for case {case_number}: {input:?}"
            );
        }
    }
}
