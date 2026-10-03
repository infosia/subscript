//! Exhaustive witness index; expected variants do not read the production map.

use crate::ambient::ApiRejectionId;
use crate::check::rejection::RejectionSite;
use crate::divergence::Divergence;

pub(super) enum WitnessEntry {
    Accepted {
        file: &'static str,
        variant: Divergence,
    },
    RejectedOnly {
        file: &'static str,
        reason: &'static str,
    },
    Unreachable {
        file: &'static str,
        reason: &'static str,
    },
}

impl WitnessEntry {
    pub(super) fn key(&self) -> (&'static str, Option<Divergence>) {
        match self {
            Self::Accepted { file, variant } => (file, Some(*variant)),
            Self::RejectedOnly { file, reason } | Self::Unreachable { file, reason } => {
                assert!(!reason.is_empty(), "no-variant entry needs a reason");
                (file, None)
            }
        }
    }
}

pub(super) fn witness_key(site: RejectionSite) -> (&'static str, Option<Divergence>) {
    witness_entry(site).key()
}

pub(super) fn witness_entry(site: RejectionSite) -> WitnessEntry {
    match site {
        RejectionSite::Api(row) => match row.id {
            ApiRejectionId::StringLocaleCompare => {
                WitnessEntry::Accepted { file: "a001.ts", variant: Divergence::LocaleSensitiveString }
            }
            ApiRejectionId::StringToLocaleUpperCase => {
                WitnessEntry::Accepted { file: "a002.ts", variant: Divergence::LocaleSensitiveString }
            }
            ApiRejectionId::StringToLocaleLowerCase => {
                WitnessEntry::Accepted { file: "a003.ts", variant: Divergence::LocaleSensitiveString }
            }
            ApiRejectionId::StringNormalize => WitnessEntry::Accepted { file: "a004.ts", variant: Divergence::UnicodeNormalization },
            ApiRejectionId::RegexStringMatch => WitnessEntry::Accepted { file: "a005.ts", variant: Divergence::MatchOptionalIndex },
            ApiRejectionId::RegexStringMatchAll => WitnessEntry::Accepted { file: "a006.ts", variant: Divergence::RegExpSubset },
            ApiRejectionId::ArrayFind => WitnessEntry::Accepted { file: "a010.ts", variant: Divergence::ArrayMethodDefaults },
            ApiRejectionId::ArrayFindLast => WitnessEntry::Accepted { file: "a011.ts", variant: Divergence::ArrayMethodDefaults },
            ApiRejectionId::ArrayFlat => WitnessEntry::Accepted { file: "a012.ts", variant: Divergence::ArrayFlattenDepth },
            ApiRejectionId::ArrayFlatMap => WitnessEntry::Accepted { file: "a013.ts", variant: Divergence::MethodTypeDomain },
            ApiRejectionId::ArrayEntries => WitnessEntry::Accepted { file: "a014.ts", variant: Divergence::NoTupleType },
            ApiRejectionId::ArrayKeys => WitnessEntry::Accepted { file: "a015.ts", variant: Divergence::IteratorTemporary },
            ApiRejectionId::ArrayValues => WitnessEntry::Accepted { file: "a016.ts", variant: Divergence::IteratorTemporary },
            ApiRejectionId::DateLocalGetFullYear => WitnessEntry::Accepted { file: "a017.ts", variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetMonth => WitnessEntry::Accepted { file: "a018.ts", variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetDate => WitnessEntry::Accepted { file: "a019.ts", variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetDay => WitnessEntry::Accepted { file: "a020.ts", variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetHours => WitnessEntry::Accepted { file: "a021.ts", variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetMinutes => WitnessEntry::Accepted { file: "a022.ts", variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetSeconds => WitnessEntry::Accepted { file: "a023.ts", variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetMilliseconds => WitnessEntry::Accepted { file: "a024.ts", variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetTimezoneOffset => WitnessEntry::Accepted { file: "a025.ts", variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetYear => WitnessEntry::RejectedOnly { file: "a026.ts", reason: "The ES2022 Date interface has no getYear member (TS2339)." },
            ApiRejectionId::DateStringToString => WitnessEntry::Accepted { file: "a027.ts", variant: Divergence::DateSubset },
            ApiRejectionId::DateStringToDateString => WitnessEntry::Accepted { file: "a028.ts", variant: Divergence::DateSubset },
            ApiRejectionId::DateStringToTimeString => WitnessEntry::Accepted { file: "a029.ts", variant: Divergence::DateSubset },
            ApiRejectionId::DateStringToLocaleString => WitnessEntry::Accepted { file: "a030.ts", variant: Divergence::DateSubset },
            ApiRejectionId::DateStringToLocaleDateString => {
                WitnessEntry::Accepted { file: "a031.ts", variant: Divergence::DateSubset }
            }
            ApiRejectionId::DateStringToLocaleTimeString => {
                WitnessEntry::Accepted { file: "a032.ts", variant: Divergence::DateSubset }
            }
            ApiRejectionId::MapKeys => WitnessEntry::Accepted { file: "a033.ts", variant: Divergence::IteratorTemporary },
            ApiRejectionId::MapValues => WitnessEntry::Accepted { file: "a034.ts", variant: Divergence::IteratorTemporary },
            ApiRejectionId::MapEntries => WitnessEntry::Accepted { file: "a035.ts", variant: Divergence::NoTupleType },
            ApiRejectionId::SetKeys => WitnessEntry::Accepted { file: "a036.ts", variant: Divergence::IteratorTemporary },
            ApiRejectionId::SetValues => WitnessEntry::Accepted { file: "a037.ts", variant: Divergence::IteratorTemporary },
            ApiRejectionId::SetEntries => WitnessEntry::Accepted { file: "a038.ts", variant: Divergence::NoTupleType },
            ApiRejectionId::JsonStringifyMapKV => WitnessEntry::Accepted { file: "a039.ts", variant: Divergence::JsonSubset },
            ApiRejectionId::JsonStringifySetK => WitnessEntry::Accepted { file: "a040.ts", variant: Divergence::JsonSubset },
            ApiRejectionId::JsonStringifyObject => WitnessEntry::Accepted { file: "a041.ts", variant: Divergence::JsonSubset },
            ApiRejectionId::JsonStringifyFunction => WitnessEntry::Accepted { file: "a042.ts", variant: Divergence::JsonSubset },
            ApiRejectionId::JsonStringifyF16 => WitnessEntry::Accepted { file: "a043.ts", variant: Divergence::JsonSubset },
            ApiRejectionId::JsonParseDateText => WitnessEntry::Accepted { file: "a045.ts", variant: Divergence::JsonSubset },
            ApiRejectionId::FormIsNaNValue => {
                WitnessEntry::Accepted { file: "a046.ts", variant: Divergence::NumberCoercionAndArguments }
            }
            ApiRejectionId::FormIsFiniteValue => {
                WitnessEntry::Accepted { file: "a047.ts", variant: Divergence::NumberCoercionAndArguments }
            }
            ApiRejectionId::FormParseIntValue => {
                WitnessEntry::Accepted { file: "a048.ts", variant: Divergence::NumberCoercionAndArguments }
            }
            ApiRejectionId::FormNumberValue => {
                WitnessEntry::Accepted { file: "a049.ts", variant: Divergence::NumberCoercionAndArguments }
            }
            ApiRejectionId::FormNewNumberValue => {
                WitnessEntry::Accepted { file: "a050.ts", variant: Divergence::NumberCoercionAndArguments }
            }
            ApiRejectionId::FormToLocaleString => {
                WitnessEntry::Accepted { file: "a051.ts", variant: Divergence::LocaleNumberFormatting }
            }
            ApiRejectionId::FormToString => {
                WitnessEntry::Accepted { file: "a052.ts", variant: Divergence::NumberCoercionAndArguments }
            }
            ApiRejectionId::FormToPrecision => {
                WitnessEntry::Accepted { file: "a053.ts", variant: Divergence::NumberCoercionAndArguments }
            }
            ApiRejectionId::FormToFixedToStringToExponentialToPrecision => {
                WitnessEntry::Accepted { file: "a054.ts", variant: Divergence::MethodTypeDomain }
            }
            ApiRejectionId::FormMaxMinHypotWithMoreThanTwoArguments => {
                WitnessEntry::Accepted { file: "a055.ts", variant: Divergence::MathSubset }
            }
            ApiRejectionId::FormMathUsedAsAValue => WitnessEntry::Accepted { file: "a056.ts", variant: Divergence::MathSubset },
            ApiRejectionId::FormDateParse => WitnessEntry::Accepted { file: "a057.ts", variant: Divergence::DateSubset },
            ApiRejectionId::FormNewDate => WitnessEntry::Accepted { file: "a058.ts", variant: Divergence::DateSubset },
            ApiRejectionId::FormNewDateYearMonth => WitnessEntry::Accepted { file: "a059.ts", variant: Divergence::DateSubset },
            ApiRejectionId::FormTemplateInterpolation => WitnessEntry::Accepted { file: "a060.ts", variant: Divergence::DateSubset },
            ApiRejectionId::FormDirectComparison => WitnessEntry::Accepted { file: "a061.ts", variant: Divergence::DateSubset },
            ApiRejectionId::FormSet => WitnessEntry::Accepted { file: "a062.ts", variant: Divergence::DateSubset },
            ApiRejectionId::FormSort => WitnessEntry::Accepted { file: "a063.ts", variant: Divergence::ArrayMethodDefaults },
            ApiRejectionId::FormReduceCallback => {
                WitnessEntry::Accepted { file: "a064.ts", variant: Divergence::ArrayMethodDefaults }
            }
            ApiRejectionId::FormReduceRightCallback => {
                WitnessEntry::Accepted { file: "a065.ts", variant: Divergence::ArrayMethodDefaults }
            }
            ApiRejectionId::FormCallbackValueIndexArray => {
                WitnessEntry::Accepted { file: "a066.ts", variant: Divergence::EscapingCapture }
            }
            ApiRejectionId::FormSpliceStartDeleteCountItems => {
                WitnessEntry::Accepted { file: "a067.ts", variant: Divergence::VariadicArguments }
            }
            ApiRejectionId::FormUnshiftValueValues => {
                WitnessEntry::Accepted { file: "a068.ts", variant: Divergence::VariadicArguments }
            }
            ApiRejectionId::FormNonCallbackTMethods => WitnessEntry::RejectedOnly { file: "a069.ts", reason: "The prelude FixedArray interface has no non-callback array methods (TS2339)." },
            ApiRejectionId::MapScalarGet => WitnessEntry::Accepted { file: "a070.ts", variant: Divergence::MapScalarGet },
            ApiRejectionId::MapNonNullableGet => WitnessEntry::Accepted { file: "a071.ts", variant: Divergence::MapNonNullableGet },
            ApiRejectionId::FormNewMapIterable => WitnessEntry::Accepted { file: "a072.ts", variant: Divergence::NoTupleType },
            ApiRejectionId::FormNewSetMap => WitnessEntry::Accepted { file: "a073.ts", variant: Divergence::NoTupleType },
            ApiRejectionId::FormNewSetGeneratorT => {
                WitnessEntry::Accepted { file: "a074.ts", variant: Divergence::GeneratorSingleUse }
            }
            ApiRejectionId::FormArrayUsedAsAValue => {
                WitnessEntry::Accepted { file: "a075.ts", variant: Divergence::CompilerOwnedValue }
            }
            ApiRejectionId::FormArrayFromSourceMapFn => {
                WitnessEntry::Accepted { file: "a076.ts", variant: Divergence::ArrayFromMapper }
            }
            ApiRejectionId::FormArrayFromMap => WitnessEntry::Accepted { file: "a077.ts", variant: Divergence::BareMapToArray },
            ApiRejectionId::FormArrayFromGeneratorT => {
                WitnessEntry::Accepted { file: "a078.ts", variant: Divergence::GeneratorSingleUse }
            }
            ApiRejectionId::FormIsArrayValue => WitnessEntry::Accepted { file: "a079.ts", variant: Divergence::ArrayIsArray },
            ApiRejectionId::FormOfValue => WitnessEntry::Accepted { file: "a080.ts", variant: Divergence::ArrayOfArity },
            ApiRejectionId::FormNewArrayLength => {
                WitnessEntry::Accepted { file: "a081.ts", variant: Divergence::ArrayHoleConstruction }
            }
            ApiRejectionId::FormGroupBy => WitnessEntry::RejectedOnly { file: "a082.ts", reason: "The ES2022 ObjectConstructor interface has no groupBy member (TS2550)." },
            ApiRejectionId::FormAlgebraNonSet => WitnessEntry::Accepted { file: "a083.ts", variant: Divergence::SetAlgebraDomain },
        },
        RejectionSite::MirrorParameter => {
            WitnessEntry::Accepted { file: "mirror-main.ts", variant: Divergence::MirrorParameterPattern }
        }
        RejectionSite::ArraySpreadFixedArray => WitnessEntry::Accepted { file: "s001.ts", variant: Divergence::FixedArraySpread },
        RejectionSite::ArraySpreadMap => WitnessEntry::Accepted { file: "s002.ts", variant: Divergence::BareMapToArray },
        RejectionSite::ArraySpreadGenerator => WitnessEntry::Accepted { file: "s003.ts", variant: Divergence::GeneratorSingleUse },
        RejectionSite::ArraySpreadSource => WitnessEntry::Accepted { file: "s004.ts", variant: Divergence::UserIterationProtocol },
        RejectionSite::MapCopyKey => WitnessEntry::Accepted { file: "s005.ts", variant: Divergence::MapKeyKind },
        RejectionSite::ContextBytesMissingType => {
            WitnessEntry::Accepted { file: "s006.ts", variant: Divergence::ExplicitIntrinsicTypeArguments }
        }
        RejectionSite::ContextBytesTypeCount => WitnessEntry::RejectedOnly { file: "s007.ts", reason: "The prelude Context.bytesOf signature permits exactly one type argument (TS2558)." },
        RejectionSite::ContextBytesArgumentCount => WitnessEntry::RejectedOnly { file: "s008.ts", reason: "The prelude Context.bytesOf signature requires exactly one value argument (TS2554)." },
        RejectionSite::ContextBytesSpread => WitnessEntry::Accepted { file: "s009.ts", variant: Divergence::VariadicArguments },
        RejectionSite::CallSpread => WitnessEntry::Accepted { file: "s010.ts", variant: Divergence::VariadicArguments },
        RejectionSite::SetSourceSpread => WitnessEntry::Accepted { file: "s011.ts", variant: Divergence::VariadicArguments },
        RejectionSite::SetSourceDomain => WitnessEntry::Accepted { file: "s012.ts", variant: Divergence::SourceConstructionDomain },
        RejectionSite::NewMapSetKey => WitnessEntry::Accepted { file: "s013.ts", variant: Divergence::MapKeyKind },
        RejectionSite::RegexSticky => WitnessEntry::Accepted { file: "s014.ts", variant: Divergence::RegExpSubset },
        RejectionSite::ContextValue => WitnessEntry::Accepted { file: "s015.ts", variant: Divergence::CompilerOwnedValue },
        RejectionSite::NumberValue => WitnessEntry::Accepted { file: "s016.ts", variant: Divergence::CompilerOwnedValue },
        RejectionSite::JsonValue => WitnessEntry::Accepted { file: "s017.ts", variant: Divergence::CompilerOwnedValue },
        RejectionSite::DateValue => WitnessEntry::Accepted { file: "s018.ts", variant: Divergence::CompilerOwnedValue },
        RejectionSite::MapSetValue => WitnessEntry::Accepted { file: "s019.ts", variant: Divergence::CompilerOwnedValue },
        RejectionSite::NumberGlobalValue => WitnessEntry::Accepted { file: "s020.ts", variant: Divergence::CompilerOwnedValue },
        RejectionSite::CoercingGlobalValue => {
            WitnessEntry::Accepted { file: "s021.ts", variant: Divergence::NumberCoercionAndArguments }
        }
        RejectionSite::RegexMember => WitnessEntry::Accepted { file: "s022.ts", variant: Divergence::RegExpSubset },
        RejectionSite::DateMemberWrite => WitnessEntry::Accepted { file: "s023.ts", variant: Divergence::DateSubset },
        RejectionSite::DateMethodValue => WitnessEntry::Accepted { file: "s024.ts", variant: Divergence::CompilerOwnedValue },
        RejectionSite::NumberMethodValue => WitnessEntry::Accepted { file: "s025.ts", variant: Divergence::CompilerOwnedValue },
        RejectionSite::NumberMethodArgumentCount => WitnessEntry::RejectedOnly { file: "s026.ts", reason: "The ES2022 numeric formatting signatures permit at most one argument (TS2554)." },
        RejectionSite::ArrayElementDomain => WitnessEntry::Accepted { file: "s027.ts", variant: Divergence::MethodTypeDomain },
        RejectionSite::ArrayJoinDomain => WitnessEntry::Accepted { file: "s028.ts", variant: Divergence::ArrayJoinDomain },
        RejectionSite::ArrayCallbackSpread => WitnessEntry::Accepted { file: "s029.ts", variant: Divergence::VariadicArguments },
        RejectionSite::ArrayAccumulatorDomain => WitnessEntry::Accepted { file: "s030.ts", variant: Divergence::MethodTypeDomain },
        RejectionSite::ArrayMapResult => WitnessEntry::Accepted { file: "s031.ts", variant: Divergence::MethodTypeDomain },
        RejectionSite::MapGroupByKey => WitnessEntry::Accepted { file: "s032.ts", variant: Divergence::MapKeyKind },
        RejectionSite::ArrayStaticMember => WitnessEntry::Accepted { file: "s033.ts", variant: Divergence::NamespaceObjectMember },
        RejectionSite::ArrayFromTypeCount => {
            WitnessEntry::Accepted { file: "s034.ts", variant: Divergence::ExplicitIntrinsicTypeArguments }
        }
        RejectionSite::ArrayFromArgumentCount => WitnessEntry::RejectedOnly { file: "s035.ts", reason: "The mapper guard takes two or three arguments; all other non-single counts violate Array.from overloads (TS2554)." },
        RejectionSite::ArrayFromSpread => WitnessEntry::Accepted { file: "s036.ts", variant: Divergence::VariadicArguments },
        RejectionSite::ArrayFromSource => WitnessEntry::Accepted { file: "s037.ts", variant: Divergence::SourceConstructionDomain },
        RejectionSite::CallbackParameterCount => {
            WitnessEntry::Accepted { file: "s038.ts", variant: Divergence::CallbackParameterShape }
        }
        RejectionSite::ContextMember => WitnessEntry::Accepted { file: "s039.ts", variant: Divergence::CompilerOwnedValue },
        RejectionSite::JsonMember => WitnessEntry::Accepted { file: "s040.ts", variant: Divergence::CompilerOwnedValue },
        RejectionSite::ArrayFromValue => WitnessEntry::Accepted { file: "s041.ts", variant: Divergence::CompilerOwnedValue },
        RejectionSite::ArrayMember => WitnessEntry::Accepted { file: "s042.ts", variant: Divergence::NamespaceObjectMember },
        RejectionSite::MapGroupByValue => WitnessEntry::Accepted { file: "s043.ts", variant: Divergence::CompilerOwnedValue },
        RejectionSite::MapSetMember => WitnessEntry::Accepted { file: "s044.ts", variant: Divergence::NamespaceObjectMember },
        RejectionSite::MathMemberWrite => WitnessEntry::Accepted { file: "s045.ts", variant: Divergence::MathSubset },
        RejectionSite::MathMethodValue => WitnessEntry::Accepted { file: "s046.ts", variant: Divergence::MathSubset },
        RejectionSite::MathMember => WitnessEntry::Accepted { file: "s047.ts", variant: Divergence::MathSubset },
        RejectionSite::MathCallCount => WitnessEntry::Accepted { file: "s048.ts", variant: Divergence::MathSubset },
        RejectionSite::NumberMemberWrite => WitnessEntry::Accepted { file: "s049.ts", variant: Divergence::CompilerOwnedValue },
        RejectionSite::NumberStaticValue => WitnessEntry::Accepted { file: "s050.ts", variant: Divergence::CompilerOwnedValue },
        RejectionSite::NumberMember => WitnessEntry::Accepted { file: "s051.ts", variant: Divergence::NamespaceObjectMember },
        RejectionSite::NumberPredicateCount => WitnessEntry::RejectedOnly { file: "s052.ts", reason: "The ES2022 Number predicate signatures require exactly one argument (TS2554)." },
        RejectionSite::NumberGlobalCount => WitnessEntry::Accepted { file: "s053.ts", variant: Divergence::NumberCoercionAndArguments },
        RejectionSite::RegexExec => WitnessEntry::Accepted { file: "s054.ts", variant: Divergence::RegExpSubset },
        RejectionSite::StringPatternSpread => WitnessEntry::Accepted { file: "s055.ts", variant: Divergence::VariadicArguments },
        RejectionSite::StringSearchPattern => WitnessEntry::Accepted { file: "s056.ts", variant: Divergence::StringSearchPattern },
        RejectionSite::DateStaticWrite => WitnessEntry::Accepted { file: "s057.ts", variant: Divergence::CompilerOwnedValue },
        RejectionSite::DateStaticValue => WitnessEntry::Accepted { file: "s058.ts", variant: Divergence::CompilerOwnedValue },
        RejectionSite::DateNewSpread => WitnessEntry::Accepted { file: "s059.ts", variant: Divergence::VariadicArguments },
        RejectionSite::DateMember => WitnessEntry::Accepted { file: "s060.ts", variant: Divergence::DateSubset },
        RejectionSite::Float16Unary => WitnessEntry::Accepted { file: "s061.ts", variant: Divergence::StorageOnlyFloat16 },
        RejectionSite::JsonStaticMember => WitnessEntry::Accepted { file: "s062.ts", variant: Divergence::NamespaceObjectMember },
        RejectionSite::JsonStringifyCount => WitnessEntry::Accepted { file: "s063.ts", variant: Divergence::JsonCallArguments },
        RejectionSite::JsonStringifySpread => WitnessEntry::Accepted { file: "s064.ts", variant: Divergence::VariadicArguments },
        RejectionSite::JsonStringifyDomain => WitnessEntry::Accepted { file: "s065.ts", variant: Divergence::JsonTypeDomain },
        RejectionSite::JsonStringifyHelper => WitnessEntry::Unreachable { file: "s066.ts", reason: "The json_serializable guard and collect_json_types graph closure admit only serializer helper types." },
        RejectionSite::JsonParseCount => WitnessEntry::Accepted { file: "s067.ts", variant: Divergence::JsonCallArguments },
        RejectionSite::JsonParseSpread => WitnessEntry::Accepted { file: "s068.ts", variant: Divergence::VariadicArguments },
        RejectionSite::JsonParseTypeCount => WitnessEntry::RejectedOnly { file: "s069.ts", reason: "The prelude JSON.parse overload permits zero or one type argument (TS2558)." },
        RejectionSite::JsonParseTarget => WitnessEntry::Accepted { file: "s070.ts", variant: Divergence::JsonSubset },
        RejectionSite::JsonParseDomain => WitnessEntry::Accepted { file: "s071.ts", variant: Divergence::JsonTypeDomain },
        RejectionSite::JsonParseHelper => WitnessEntry::Unreachable { file: "s072.ts", reason: "The serializable, Error, and Date guards plus graph closure admit only parser helper types." },
        RejectionSite::JsonError => WitnessEntry::Accepted { file: "s073.ts", variant: Divergence::JsonSubset },
        RejectionSite::ForOfEntries => WitnessEntry::Accepted { file: "s074.ts", variant: Divergence::NoTupleType },
        RejectionSite::ForOfKeys => WitnessEntry::RejectedOnly { file: "s075.ts", reason: "The fused receiver guard leaves only FixedArray here; its prelude interface has no keys or values member (TS2339)." },
        RejectionSite::ForOfMap => WitnessEntry::Accepted { file: "s076.ts", variant: Divergence::BareMapSubject },
        RejectionSite::ForOfUserClass => WitnessEntry::Accepted { file: "s077.ts", variant: Divergence::UserIterationProtocol },
        RejectionSite::ForOfSubject => WitnessEntry::RejectedOnly { file: "s078.ts", reason: "The resolved-type guard accepts containers and routes classes elsewhere; remaining scalar or nullable subjects lack a non-null iteration protocol (TS2488/TS18047)." },
        RejectionSite::RegexMatchType => WitnessEntry::Accepted { file: "s079.ts", variant: Divergence::RegExpSubset },
        RejectionSite::MapSetTypeKey => WitnessEntry::Accepted { file: "s080.ts", variant: Divergence::MapKeyKind },
        RejectionSite::Float16Update => WitnessEntry::Accepted { file: "s081.ts", variant: Divergence::StorageOnlyFloat16 },
        RejectionSite::Float16Binary => WitnessEntry::Accepted { file: "s082.ts", variant: Divergence::StorageOnlyFloat16 },
    }
}

pub(super) const DIRECT_SITES: &[RejectionSite] = &[
    RejectionSite::ArraySpreadFixedArray,
    RejectionSite::ArraySpreadMap,
    RejectionSite::ArraySpreadGenerator,
    RejectionSite::ArraySpreadSource,
    RejectionSite::MapCopyKey,
    RejectionSite::ContextBytesMissingType,
    RejectionSite::ContextBytesTypeCount,
    RejectionSite::ContextBytesArgumentCount,
    RejectionSite::ContextBytesSpread,
    RejectionSite::CallSpread,
    RejectionSite::SetSourceSpread,
    RejectionSite::SetSourceDomain,
    RejectionSite::NewMapSetKey,
    RejectionSite::RegexSticky,
    RejectionSite::ContextValue,
    RejectionSite::NumberValue,
    RejectionSite::JsonValue,
    RejectionSite::DateValue,
    RejectionSite::MapSetValue,
    RejectionSite::NumberGlobalValue,
    RejectionSite::CoercingGlobalValue,
    RejectionSite::RegexMember,
    RejectionSite::DateMemberWrite,
    RejectionSite::DateMethodValue,
    RejectionSite::NumberMethodValue,
    RejectionSite::NumberMethodArgumentCount,
    RejectionSite::ArrayElementDomain,
    RejectionSite::ArrayJoinDomain,
    RejectionSite::ArrayCallbackSpread,
    RejectionSite::ArrayAccumulatorDomain,
    RejectionSite::ArrayMapResult,
    RejectionSite::MapGroupByKey,
    RejectionSite::ArrayStaticMember,
    RejectionSite::ArrayFromTypeCount,
    RejectionSite::ArrayFromArgumentCount,
    RejectionSite::ArrayFromSpread,
    RejectionSite::ArrayFromSource,
    RejectionSite::CallbackParameterCount,
    RejectionSite::ContextMember,
    RejectionSite::JsonMember,
    RejectionSite::ArrayFromValue,
    RejectionSite::ArrayMember,
    RejectionSite::MapGroupByValue,
    RejectionSite::MapSetMember,
    RejectionSite::MathMemberWrite,
    RejectionSite::MathMethodValue,
    RejectionSite::MathMember,
    RejectionSite::MathCallCount,
    RejectionSite::NumberMemberWrite,
    RejectionSite::NumberStaticValue,
    RejectionSite::NumberMember,
    RejectionSite::NumberPredicateCount,
    RejectionSite::NumberGlobalCount,
    RejectionSite::RegexExec,
    RejectionSite::StringPatternSpread,
    RejectionSite::StringSearchPattern,
    RejectionSite::DateStaticWrite,
    RejectionSite::DateStaticValue,
    RejectionSite::DateNewSpread,
    RejectionSite::DateMember,
    RejectionSite::Float16Unary,
    RejectionSite::JsonStaticMember,
    RejectionSite::JsonStringifyCount,
    RejectionSite::JsonStringifySpread,
    RejectionSite::JsonStringifyDomain,
    RejectionSite::JsonStringifyHelper,
    RejectionSite::JsonParseCount,
    RejectionSite::JsonParseSpread,
    RejectionSite::JsonParseTypeCount,
    RejectionSite::JsonParseTarget,
    RejectionSite::JsonParseDomain,
    RejectionSite::JsonParseHelper,
    RejectionSite::JsonError,
    RejectionSite::ForOfEntries,
    RejectionSite::ForOfKeys,
    RejectionSite::ForOfMap,
    RejectionSite::ForOfUserClass,
    RejectionSite::ForOfSubject,
    RejectionSite::RegexMatchType,
    RejectionSite::MapSetTypeKey,
    RejectionSite::Float16Update,
    RejectionSite::Float16Binary,
    RejectionSite::MirrorParameter,
];

pub(super) const NEW_VARIANTS: &[Divergence] = &[
    Divergence::CompilerOwnedValue,
    Divergence::NamespaceObjectMember,
    Divergence::UnicodeNormalization,
    Divergence::MatchOptionalIndex,
    Divergence::ArrayFlattenDepth,
    Divergence::MethodTypeDomain,
    Divergence::ArrayJoinDomain,
    Divergence::FixedArraySpread,
    Divergence::ExplicitIntrinsicTypeArguments,
    Divergence::SourceConstructionDomain,
    Divergence::CallbackParameterShape,
    Divergence::JsonCallArguments,
    Divergence::JsonTypeDomain,
    Divergence::StringSearchPattern,
    Divergence::MirrorParameterPattern,
    Divergence::LocaleNumberFormatting,
    Divergence::UserIterationProtocol,
    Divergence::SetAlgebraDomain,
];
