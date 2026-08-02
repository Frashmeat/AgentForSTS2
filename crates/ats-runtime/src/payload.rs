use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use thiserror::Error;

use ats_kernel::SchemaRef;

#[derive(Debug, Error)]
pub enum PayloadError {
    #[error("versioned payload must serialize as a JSON object")]
    NotAnObject,
    #[error("versioned payload schema does not match the registered contract")]
    SchemaMismatch,
    #[error("versioned payload serialization failed")]
    Serialize(#[source] serde_json::Error),
    #[error("versioned payload decoding failed")]
    Decode(#[source] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionedPayload {
    schema: SchemaRef,
    payload: serde_json::Value,
}

impl VersionedPayload {
    pub fn from_typed<T>(schema: SchemaRef, value: &T) -> Result<Self, PayloadError>
    where
        T: Serialize,
    {
        let payload = serde_json::to_value(value).map_err(PayloadError::Serialize)?;
        Self::from_value(schema, payload)
    }

    fn from_value(schema: SchemaRef, payload: serde_json::Value) -> Result<Self, PayloadError> {
        if !payload.is_object() {
            return Err(PayloadError::NotAnObject);
        }
        Ok(Self { schema, payload })
    }

    pub fn decode<T>(&self, expected: &SchemaRef) -> Result<T, PayloadError>
    where
        T: DeserializeOwned,
    {
        if &self.schema != expected {
            return Err(PayloadError::SchemaMismatch);
        }
        serde_json::from_value(self.payload.clone()).map_err(PayloadError::Decode)
    }

    #[must_use]
    pub fn schema(&self) -> &SchemaRef {
        &self.schema
    }

    #[must_use]
    pub fn payload(&self) -> &serde_json::Value {
        &self.payload
    }
}

impl<'de> Deserialize<'de> for VersionedPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            schema: SchemaRef,
            payload: serde_json::Value,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::from_value(wire.schema, wire.payload).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use ats_kernel::{SchemaId, SchemaVersion};

    use super::*;

    #[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct Fixture {
        value: String,
    }

    fn schema(id: &str) -> SchemaRef {
        SchemaRef {
            id: SchemaId::parse(id).unwrap(),
            version: SchemaVersion::new(1).unwrap(),
        }
    }

    #[test]
    fn typed_payload_round_trips_and_decodes_with_matching_schema() {
        let expected = schema("fixture.request");
        let payload =
            VersionedPayload::from_typed(expected.clone(), &Fixture { value: "ok".into() })
                .unwrap();

        let json = serde_json::to_string(&payload).unwrap();
        let decoded: VersionedPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(
            decoded.decode::<Fixture>(&expected).unwrap(),
            Fixture { value: "ok".into() }
        );
    }

    #[test]
    fn rejects_scalar_mismatched_and_malformed_payloads() {
        assert!(matches!(
            VersionedPayload::from_typed(schema("fixture.scalar"), &"text"),
            Err(PayloadError::NotAnObject)
        ));

        let payload = VersionedPayload::from_typed(
            schema("fixture.request"),
            &Fixture { value: "ok".into() },
        )
        .unwrap();
        assert!(matches!(
            payload.decode::<Fixture>(&schema("fixture.other")),
            Err(PayloadError::SchemaMismatch)
        ));

        let malformed = r#"{
          "schema":{"id":"fixture.request","version":1},
          "payload":"not-an-object"
        }"#;
        assert!(serde_json::from_str::<VersionedPayload>(malformed).is_err());
    }
}
