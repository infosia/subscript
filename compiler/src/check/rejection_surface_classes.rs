//! Rejection classes for provenance, binding patterns, and runtime surface forms.

// Expand both topic tables into one exhaustive match over RejectionSite.
macro_rules! rejection_classes {
    ($($core:tt)*) => {
        impl RejectionSite {
            pub(crate) fn class(self) -> (RuleCode, RejectionClass) {
                use RejectionClass::{Diverges, TscRejects};
                match self {
                    $($core)*
                    Self::ProvenanceUnknownKind => {
                        (RuleCode::S100, Diverges(Divergence::ProvenanceUnknownKind))
                    }
                    Self::ProvenanceMissingKind => {
                        (RuleCode::S100, Diverges(Divergence::ProvenanceMissingKind))
                    }
                    Self::ProvenanceFieldSeparator => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceFieldSeparator),
                    ),
                    Self::ProvenanceUnexpectedKey => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceUnexpectedKey),
                    ),
                    Self::ProvenanceUnquotedString => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceUnquotedString),
                    ),
                    Self::ProvenanceUnterminatedString => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceUnterminatedString),
                    ),
                    Self::ProvenanceUnterminatedEscape => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceUnterminatedEscape),
                    ),
                    Self::ProvenanceUnsupportedEscape => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceUnsupportedEscape),
                    ),
                    Self::ProvenanceControlCharacter => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceControlCharacter),
                    ),
                    Self::ProvenanceInvalidUnicodeDigits => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceInvalidUnicodeDigits),
                    ),
                    Self::ProvenanceInvalidBoolean => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceInvalidBoolean),
                    ),
                    Self::ProvenanceTrailingData => {
                        (RuleCode::S100, Diverges(Divergence::ProvenanceTrailingData))
                    }
                    Self::InitializerSegmentRange => (RuleCode::S100, TscRejects),
                    Self::GlobalInitializerOwnerRange => (RuleCode::S100, TscRejects),
                    Self::JsonSerializerTypeKind => (RuleCode::S014, Diverges(Divergence::JsonTypeDomain)),
                    Self::JsonValidatorTypeKind => (RuleCode::S014, Diverges(Divergence::JsonTypeDomain)),
                    Self::JsonConstructorTypeKind => (RuleCode::S014, Diverges(Divergence::JsonTypeDomain)),
                    Self::JsonArrayConstructorTypeKind => {
                        (RuleCode::S014, Diverges(Divergence::JsonTypeDomain))
                    }
                    Self::JsonArrayStoreTypeKind => (RuleCode::S014, Diverges(Divergence::JsonTypeDomain)),
                    Self::RegexPattern => (RuleCode::S100, TscRejects),
                    Self::JsonMissingSyntaxError => (RuleCode::S014, Diverges(Divergence::JsonTypeDomain)),
                    Self::JsonMissingTypeError => (RuleCode::S014, Diverges(Divergence::JsonTypeDomain)),
                    Self::JsonMissingGraphType => (RuleCode::S014, Diverges(Divergence::JsonTypeDomain)),
                    Self::InitializerMissingSummary => (RuleCode::S100, TscRejects),
                    Self::InitializerMissingGlobal => (RuleCode::S100, TscRejects),
                    Self::ProvenanceInvalidUnicodeEncoding => (RuleCode::S100, TscRejects),
                    Self::ProvenanceShortUnicodeEscape => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceShortUnicodeEscape),
                    ),
                    Self::ProvenanceInvalidUnicode => (RuleCode::S100, TscRejects),
                    Self::ProvenanceInvalidUnicodeScalar => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceInvalidUnicodeScalar),
                    ),
                    Self::ProvenanceHeaderBasename => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceHeaderBasename),
                    ),
                    Self::ProvenanceDuplicateHeader => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceDuplicateHeader),
                    ),
                    Self::ProvenanceEmptyDescriptor => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceEmptyDescriptor),
                    ),
                    Self::ProvenanceDuplicateDescriptor => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceDuplicateDescriptor),
                    ),
                    Self::ProvenanceEmptyStringView => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceEmptyStringView),
                    ),
                    Self::ProvenanceDuplicateStringView => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceDuplicateStringView),
                    ),
                    Self::ProvenanceEmptyScalarPair => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceEmptyScalarPair),
                    ),
                    Self::ProvenanceDuplicateScalarPair => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceDuplicateScalarPair),
                    ),
                    Self::ProvenanceEmptyCallback => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceEmptyCallback),
                    ),
                    Self::ProvenanceDuplicateCallback => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceDuplicateCallback),
                    ),
                    Self::ProvenanceEmptyCallbackLifetime => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceEmptyCallbackLifetime),
                    ),
                    Self::ProvenanceDuplicateCallbackLifetime => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceDuplicateCallbackLifetime),
                    ),
                    Self::ProvenanceEmptyExternalType => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceEmptyExternalType),
                    ),
                    Self::ProvenanceDuplicateExternalType => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceDuplicateExternalType),
                    ),
                    Self::ProvenanceEmptyCEnum => {
                        (RuleCode::S100, Diverges(Divergence::ProvenanceEmptyCEnum))
                    }
                    Self::ProvenanceDuplicateCEnum => (
                        RuleCode::S100,
                        Diverges(Divergence::ProvenanceDuplicateCEnum),
                    ),
                    Self::BindingPatternUnsupportedRoot => {
                        (RuleCode::S100, Diverges(Divergence::NestedPattern))
                    }
                    Self::ArrayBindingNestedPattern => {
                        (RuleCode::S100, Diverges(Divergence::NestedPattern))
                    }
                    Self::ObjectBindingNestedPattern => {
                        (RuleCode::S100, Diverges(Divergence::NestedPattern))
                    }
                    Self::ArrayBindingRestElement => {
                        (RuleCode::S100, Diverges(Divergence::ArrayRestPattern))
                    }
                    Self::ArrayBindingDefaultValue => {
                        (RuleCode::S100, Diverges(Divergence::PatternDefaultValue))
                    }
                    Self::ObjectBindingShorthandDefault => {
                        (RuleCode::S100, Diverges(Divergence::PatternDefaultValue))
                    }
                    Self::ObjectBindingFieldDefault => {
                        (RuleCode::S100, Diverges(Divergence::PatternDefaultValue))
                    }
                    Self::ObjectBindingNonLiteralFieldName => {
                        (RuleCode::S100, Diverges(Divergence::PatternFieldName))
                    }
                    Self::ObjectBindingRestProperty => {
                        (RuleCode::S100, Diverges(Divergence::ObjectRestPattern))
                    }
                    Self::OptionalReceiverNonNullable => (
                        RuleCode::S100,
                        Diverges(Divergence::OptionalChainNonNullable),
                    ),
                    Self::NullishReceiverNonNullable => {
                        (RuleCode::S100, Diverges(Divergence::NullishNonNullable))
                    }
                    Self::ConditionalJoinSizedOperandWidths => {
                        (RuleCode::S100, Diverges(Divergence::SizedOperandWidths))
                    }
                    Self::ConditionalJoinGeneralUnionAndUndefined => (
                        RuleCode::S100,
                        Diverges(Divergence::GeneralUnionAndUndefined),
                    ),
                    Self::UnionLiteralUnionAlias => {
                        (RuleCode::S011, Diverges(Divergence::LiteralUnionAlias))
                    }
                    Self::UnionGeneralUnionAndUndefined => (
                        RuleCode::S011,
                        Diverges(Divergence::GeneralUnionAndUndefined),
                    ),
                    Self::AggregateAnnotationLimit => {
                        (RuleCode::S100, Diverges(Divergence::AggregateLayoutLimit))
                    }

                    Self::AssignmentLiteralAlias => {
                        (RuleCode::S100, Diverges(Divergence::LiteralUnionAlias))
                    }
                    Self::UndefinedOptionalMember => (
                        RuleCode::S012,
                        Diverges(Divergence::OptionalDescriptorMember),
                    ),
                    Self::UndefinedValue => (
                        RuleCode::S012,
                        Diverges(Divergence::GeneralUnionAndUndefined),
                    ),
                    Self::LocalAggregateFrameLimit => {
                        (RuleCode::S100, Diverges(Divergence::AggregateLayoutLimit))
                    }
                    Self::Api(_, variant) => (RuleCode::S014, Diverges(variant)),
                    Self::StatementAlwaysTruthyCondition
                    | Self::ConditionalAlwaysTruthyCondition
                    | Self::MethodValueTruthTest => (RuleCode::S100, TscRejects),
                    Self::RestParameter => (RuleCode::S100, Diverges(Divergence::RestParameter)),
                    Self::MirrorBindingPatternParameter => {
                        (RuleCode::S100, Diverges(Divergence::MirrorParameterPattern))
                    }
                    Self::ArraySpreadFixedArray => (RuleCode::S014, Diverges(Divergence::FixedArraySpread)),
                    Self::ArraySpreadMap => (RuleCode::S014, Diverges(Divergence::BareMapToArray)),
                    Self::ArraySpreadGenerator => {
                        (RuleCode::S014, Diverges(Divergence::GeneratorSingleUse))
                    }
                    Self::ArraySpreadSource => {
                        (RuleCode::S014, Diverges(Divergence::UserIterationProtocol))
                    }
                    Self::MapCopyKey => (RuleCode::S014, Diverges(Divergence::MapKeyKind)),
                    Self::ContextBytesMissingType => (
                        RuleCode::S014,
                        Diverges(Divergence::ExplicitIntrinsicTypeArguments),
                    ),
                    Self::ContextBytesTypeCount => (
                        RuleCode::S014,
                        Diverges(Divergence::ExplicitIntrinsicTypeArguments),
                    ),
                    Self::ContextBytesArgumentCount => (
                        RuleCode::S014,
                        Diverges(Divergence::ExplicitIntrinsicTypeArguments),
                    ),
                    Self::ContextBytesSpread => (RuleCode::S014, Diverges(Divergence::VariadicArguments)),
                    Self::CallSpread => (RuleCode::S014, Diverges(Divergence::VariadicArguments)),
                    Self::SetSourceSpread => (RuleCode::S014, Diverges(Divergence::VariadicArguments)),
                    Self::SetSourceDomain => (
                        RuleCode::S014,
                        Diverges(Divergence::SourceConstructionDomain),
                    ),
                    Self::NewMapSetKey => (RuleCode::S014, Diverges(Divergence::MapKeyKind)),
                    Self::RegexSticky => (RuleCode::S014, Diverges(Divergence::RegExpSubset)),
                    Self::ContextValue => (RuleCode::S014, Diverges(Divergence::CompilerOwnedValue)),
                    Self::NumberValue => (RuleCode::S014, Diverges(Divergence::CompilerOwnedValue)),
                    Self::JsonValue => (RuleCode::S014, Diverges(Divergence::CompilerOwnedValue)),
                    Self::DateValue => (RuleCode::S014, Diverges(Divergence::CompilerOwnedValue)),
                    Self::MapSetValue => (RuleCode::S014, Diverges(Divergence::CompilerOwnedValue)),
                    Self::NumberGlobalValue => (RuleCode::S014, Diverges(Divergence::CompilerOwnedValue)),
                    Self::CoercingGlobalValue => (
                        RuleCode::S014,
                        Diverges(Divergence::NumberCoercionAndArguments),
                    ),
                    Self::RegexMember => (RuleCode::S014, Diverges(Divergence::RegExpSubset)),
                    Self::DateMemberWrite => (RuleCode::S014, Diverges(Divergence::DateSubset)),
                    Self::DateMethodValue => (RuleCode::S014, Diverges(Divergence::CompilerOwnedValue)),
                    Self::NumberMethodValue => (RuleCode::S014, Diverges(Divergence::CompilerOwnedValue)),
                    Self::NumberMethodArgumentCount => (
                        RuleCode::S014,
                        Diverges(Divergence::NumberCoercionAndArguments),
                    ),
                    Self::ArrayElementDomain | Self::MapCallbackCountedValue => (RuleCode::S014, Diverges(Divergence::MethodTypeDomain)),
                    Self::ArrayJoinDomain => (RuleCode::S014, Diverges(Divergence::ArrayJoinDomain)),
                    Self::ArrayCallbackSpread => (RuleCode::S014, Diverges(Divergence::VariadicArguments)),
                    Self::ArrayAccumulatorDomain => {
                        (RuleCode::S014, Diverges(Divergence::MethodTypeDomain))
                    }
                    Self::ArrayMapResult => (RuleCode::S014, Diverges(Divergence::MethodTypeDomain)),
                    Self::MapGroupByKey => (RuleCode::S014, Diverges(Divergence::MapKeyKind)),
                    Self::ArrayStaticMember => {
                        (RuleCode::S014, Diverges(Divergence::NamespaceObjectMember))
                    }
                    Self::ArrayFromTypeCount => (
                        RuleCode::S014,
                        Diverges(Divergence::ExplicitIntrinsicTypeArguments),
                    ),
                    Self::ArrayFromArgumentCount => (RuleCode::S014, Diverges(Divergence::ArrayFromMapper)),
                    Self::ArrayFromSpread => (RuleCode::S014, Diverges(Divergence::VariadicArguments)),
                    Self::ArrayFromSource => (
                        RuleCode::S014,
                        Diverges(Divergence::SourceConstructionDomain),
                    ),
                    Self::CallbackParameterCount => {
                        (RuleCode::S014, Diverges(Divergence::CallbackParameterShape))
                    }
                    Self::ContextMember => (RuleCode::S014, Diverges(Divergence::CompilerOwnedValue)),
                    Self::JsonMember => (RuleCode::S014, Diverges(Divergence::CompilerOwnedValue)),
                    Self::ArrayFromValue => (RuleCode::S014, Diverges(Divergence::CompilerOwnedValue)),
                    Self::ArrayMember => (RuleCode::S014, Diverges(Divergence::NamespaceObjectMember)),
                    Self::MapGroupByValue => (RuleCode::S014, Diverges(Divergence::CompilerOwnedValue)),
                    Self::MapSetMember => (RuleCode::S014, Diverges(Divergence::NamespaceObjectMember)),
                    Self::MathMemberWrite => (RuleCode::S014, Diverges(Divergence::MathSubset)),
                    Self::MathMethodValue => (RuleCode::S014, Diverges(Divergence::MathSubset)),
                    Self::MathMember => (RuleCode::S014, Diverges(Divergence::MathSubset)),
                    Self::MathCallCount => (RuleCode::S014, Diverges(Divergence::MathSubset)),
                    Self::NumberMemberWrite => (RuleCode::S014, Diverges(Divergence::CompilerOwnedValue)),
                    Self::NumberStaticValue => (RuleCode::S014, Diverges(Divergence::CompilerOwnedValue)),
                    Self::NumberMember => (RuleCode::S014, Diverges(Divergence::NamespaceObjectMember)),
                    Self::NumberPredicateCount => (
                        RuleCode::S014,
                        Diverges(Divergence::NumberCoercionAndArguments),
                    ),
                    Self::NumberGlobalCount => (
                        RuleCode::S014,
                        Diverges(Divergence::NumberCoercionAndArguments),
                    ),
                    Self::RegexExec => (RuleCode::S014, Diverges(Divergence::RegExpSubset)),
                    Self::StringPatternSpread => (RuleCode::S014, Diverges(Divergence::VariadicArguments)),
                    Self::StringSearchPattern => {
                        (RuleCode::S014, Diverges(Divergence::StringSearchPattern))
                    }
                    Self::DateStaticWrite => (RuleCode::S014, Diverges(Divergence::CompilerOwnedValue)),
                    Self::DateStaticValue => (RuleCode::S014, Diverges(Divergence::CompilerOwnedValue)),
                    Self::DateNewSpread => (RuleCode::S014, Diverges(Divergence::VariadicArguments)),
                    Self::DateMember => (RuleCode::S014, Diverges(Divergence::DateSubset)),
                    Self::HalfFloatUnary => (RuleCode::S014, Diverges(Divergence::StorageOnlyHalfFloat)),
                    Self::JsonStaticMember => (RuleCode::S014, Diverges(Divergence::NamespaceObjectMember)),
                    Self::JsonStringifyCount => (RuleCode::S014, Diverges(Divergence::JsonCallArguments)),
                    Self::JsonStringifySpread => (RuleCode::S014, Diverges(Divergence::VariadicArguments)),
                    Self::JsonStringifyDomain => (RuleCode::S014, Diverges(Divergence::JsonTypeDomain)),
                    Self::JsonParseCount => (RuleCode::S014, Diverges(Divergence::JsonCallArguments)),
                    Self::JsonParseSpread => (RuleCode::S014, Diverges(Divergence::VariadicArguments)),
                    Self::JsonParseTypeCount => (RuleCode::S014, Diverges(Divergence::JsonSubset)),
                    Self::JsonParseTarget => (RuleCode::S014, Diverges(Divergence::JsonSubset)),
                    Self::JsonParseDomain => (RuleCode::S014, Diverges(Divergence::JsonTypeDomain)),
                    Self::JsonError => (RuleCode::S014, Diverges(Divergence::JsonSubset)),
                    Self::ForOfEntries => (RuleCode::S014, Diverges(Divergence::NoTupleType)),
                    Self::ForOfKeys => (RuleCode::S014, Diverges(Divergence::IteratorTemporary)),
                    Self::ForOfMap => (RuleCode::S014, Diverges(Divergence::BareMapSubject)),
                    Self::ForOfUserClass => (RuleCode::S014, Diverges(Divergence::UserIterationProtocol)),
                    Self::ForOfSubject => (RuleCode::S014, Diverges(Divergence::IterationSubjectDomain)),
                    Self::RegexMatchType => (RuleCode::S014, Diverges(Divergence::RegExpSubset)),
                    Self::MapSetTypeKey => (RuleCode::S014, Diverges(Divergence::MapKeyKind)),
                    Self::HalfFloatUpdate => (RuleCode::S014, Diverges(Divergence::StorageOnlyHalfFloat)),
                    Self::HalfFloatBinary => (RuleCode::S014, Diverges(Divergence::StorageOnlyHalfFloat)),
                }
            }
        }
    };
}
