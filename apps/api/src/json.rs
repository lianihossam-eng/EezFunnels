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
mod tests {
    use super::{validate_unique_json, JsonValidationError};

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
}
