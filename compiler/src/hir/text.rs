//! Text runtime identities for stdlib.md §19.

/// Error formatting and URI runtime operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TextFn {
    /// Formats the Error name and message.
    ErrorToString,
    /// Encodes a complete URI.
    EncodeUri,
    /// Encodes one URI component.
    EncodeComponent,
    /// Decodes a complete URI.
    DecodeUri,
    /// Decodes one URI component.
    DecodeComponent,
    /// Returns the decodeURI failure message, or an empty string.
    UriFailure,
    /// Returns the decodeURIComponent failure message, or an empty string.
    ComponentFailure,
}

impl TextFn {
    /// Stable runtime declaration order.
    pub const ALL: [Self; 7] = [
        Self::ErrorToString,
        Self::EncodeUri,
        Self::EncodeComponent,
        Self::DecodeUri,
        Self::DecodeComponent,
        Self::UriFailure,
        Self::ComponentFailure,
    ];

    /// The runtime C symbol.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::ErrorToString => "subscript_rt_error_to_string",
            Self::EncodeUri => "subscript_rt_encode_uri",
            Self::EncodeComponent => "subscript_rt_encode_uri_component",
            Self::DecodeUri => "subscript_rt_decode_uri",
            Self::DecodeComponent => "subscript_rt_decode_uri_component",
            Self::UriFailure => "subscript_rt_decode_uri_failure",
            Self::ComponentFailure => "subscript_rt_decode_uri_component_failure",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn symbols_are_unique_and_complete() {
        let symbols: std::collections::HashSet<_> =
            TextFn::ALL.into_iter().map(TextFn::symbol).collect();
        assert_eq!(symbols.len(), 7);
        assert!(symbols.contains("subscript_rt_error_to_string"));
        assert!(symbols.contains("subscript_rt_decode_uri_component"));
    }
}
