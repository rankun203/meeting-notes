//! Serde helpers shared across service inputs.

/// Tri-state deserializer for `Option<Option<T>>` fields that need to
/// distinguish "don't touch" (field absent) from "clear" (field present
/// with value `null`) from "set to X" (field present with a value).
///
/// Usage:
/// ```ignore
/// #[derive(Deserialize)]
/// struct Patch {
///     #[serde(default, deserialize_with = "crate::services::serde_helpers::double_option")]
///     notes: Option<Option<String>>,
/// }
/// ```
pub fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    Option::<T>::deserialize(deserializer).map(Some)
}
