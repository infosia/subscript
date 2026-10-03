//! Exhaustive witness index; expected variants do not read the production map.

use crate::ambient::ApiRejectionId;
use crate::check::rejection::RejectionSite;
use crate::divergence::Divergence;

pub(super) enum WitnessEntry {
    Reachable {
        files: &'static [&'static str],
        variant: Divergence,
    },
    Unreachable {
        files: &'static [&'static str],
        variant: Divergence,
        reason: &'static str,
    },
}

impl WitnessEntry {
    pub(super) fn files(&self) -> &'static [&'static str] {
        match self {
            Self::Reachable { files, .. } | Self::Unreachable { files, .. } => files,
        }
    }

    pub(super) fn key(&self) -> (&'static str, Divergence) {
        match self {
            Self::Reachable { files, variant } => (files[0], *variant),
            Self::Unreachable {
                files,
                variant,
                reason,
            } => {
                assert!(!reason.is_empty(), "unreachable entry needs a reason");
                (files[0], *variant)
            }
        }
    }
}

pub(super) fn witness_key(site: RejectionSite) -> (&'static str, Divergence) {
    witness_entry(site).key()
}

pub(super) fn witness_entry(site: RejectionSite) -> WitnessEntry {
    match site {
        RejectionSite::Api(row) => match row.id {
            ApiRejectionId::StringLocaleCompare => {
                WitnessEntry::Reachable { files: &["a001.ts", "a001-b.ts"], variant: Divergence::LocaleSensitiveString }
            }
            ApiRejectionId::StringToLocaleUpperCase => {
                WitnessEntry::Reachable { files: &["a002.ts", "a002-b.ts"], variant: Divergence::LocaleSensitiveString }
            }
            ApiRejectionId::StringToLocaleLowerCase => {
                WitnessEntry::Reachable { files: &["a003.ts", "a003-b.ts"], variant: Divergence::LocaleSensitiveString }
            }
            ApiRejectionId::StringNormalize => WitnessEntry::Reachable { files: &["a004.ts", "a004-b.ts"], variant: Divergence::UnicodeNormalization },
            ApiRejectionId::RegexStringMatch => WitnessEntry::Reachable { files: &["a005.ts", "a005-b.ts", "r27-string-match-old.ts"], variant: Divergence::MatchOptionalIndex },
            ApiRejectionId::RegexStringMatchAll => WitnessEntry::Reachable { files: &["a006.ts", "a006-b.ts"], variant: Divergence::RegExpSubset },
            ApiRejectionId::ArrayFind => WitnessEntry::Reachable { files: &["a010.ts", "a010-b.ts"], variant: Divergence::ArrayMethodDefaults },
            ApiRejectionId::ArrayFindLast => WitnessEntry::Reachable { files: &["a011.ts", "a011-b.ts"], variant: Divergence::ArrayMethodDefaults },
            ApiRejectionId::ArrayFlat => WitnessEntry::Reachable { files: &["a012.ts", "a012-b.ts"], variant: Divergence::ArrayFlattenDepth },
            ApiRejectionId::ArrayFlatMap => WitnessEntry::Reachable { files: &["a013.ts", "a013-c.ts"], variant: Divergence::MethodTypeDomain },
            ApiRejectionId::ArrayEntries => WitnessEntry::Reachable { files: &["a014.ts", "a014-b.ts"], variant: Divergence::NoTupleType },
            ApiRejectionId::ArrayKeys => WitnessEntry::Reachable { files: &["a015.ts", "a015-b.ts"], variant: Divergence::IteratorTemporary },
            ApiRejectionId::ArrayValues => WitnessEntry::Reachable { files: &["a016.ts", "a016-b.ts"], variant: Divergence::IteratorTemporary },
            ApiRejectionId::DateLocalGetFullYear => WitnessEntry::Reachable { files: &["a017.ts", "a017-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetMonth => WitnessEntry::Reachable { files: &["a018.ts", "a018-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetDate => WitnessEntry::Reachable { files: &["a019.ts", "a019-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetDay => WitnessEntry::Reachable { files: &["a020.ts", "a020-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetHours => WitnessEntry::Reachable { files: &["a021.ts", "a021-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetMinutes => WitnessEntry::Reachable { files: &["a022.ts", "a022-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetSeconds => WitnessEntry::Reachable { files: &["a023.ts", "a023-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetMilliseconds => WitnessEntry::Reachable { files: &["a024.ts", "a024-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetTimezoneOffset => WitnessEntry::Reachable { files: &["a025.ts", "a025-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::DateLocalGetYear => WitnessEntry::Reachable { files: &["a026.ts", "a026-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::DateStringToString => WitnessEntry::Reachable { files: &["a027.ts", "a027-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::DateStringToDateString => WitnessEntry::Reachable { files: &["a028.ts", "a028-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::DateStringToTimeString => WitnessEntry::Reachable { files: &["a029.ts", "a029-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::DateStringToLocaleString => WitnessEntry::Reachable { files: &["a030.ts", "a030-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::DateStringToLocaleDateString => {
                WitnessEntry::Reachable { files: &["a031.ts", "a031-b.ts"], variant: Divergence::DateSubset }
            }
            ApiRejectionId::DateStringToLocaleTimeString => {
                WitnessEntry::Reachable { files: &["a032.ts", "a032-b.ts"], variant: Divergence::DateSubset }
            }
            ApiRejectionId::MapKeys => WitnessEntry::Reachable { files: &["a033.ts", "a033-b.ts", "r76-return-keys-view-old.ts", "r77-pass-keys-view-old.ts"], variant: Divergence::IteratorTemporary },
            ApiRejectionId::MapValues => WitnessEntry::Reachable { files: &["a034.ts", "a034-b.ts"], variant: Divergence::IteratorTemporary },
            ApiRejectionId::MapEntries => WitnessEntry::Reachable { files: &["a035.ts", "a035-b.ts"], variant: Divergence::NoTupleType },
            ApiRejectionId::SetKeys => WitnessEntry::Reachable { files: &["a036.ts", "a036-b.ts"], variant: Divergence::IteratorTemporary },
            ApiRejectionId::SetValues => WitnessEntry::Reachable { files: &["a037.ts", "a037-b.ts"], variant: Divergence::IteratorTemporary },
            ApiRejectionId::SetEntries => WitnessEntry::Reachable { files: &["a038.ts", "a038-b.ts"], variant: Divergence::NoTupleType },
            ApiRejectionId::JsonStringifyMapKV => WitnessEntry::Reachable { files: &["a039.ts", "a039-b.ts"], variant: Divergence::JsonSubset },
            ApiRejectionId::JsonStringifySetK => WitnessEntry::Reachable { files: &["a040.ts", "a040-b.ts"], variant: Divergence::JsonSubset },
            ApiRejectionId::JsonStringifyObject => WitnessEntry::Reachable { files: &["a041.ts", "a041-b.ts"], variant: Divergence::JsonSubset },
            ApiRejectionId::JsonStringifyFunction => WitnessEntry::Reachable { files: &["a042.ts", "a042-b.ts"], variant: Divergence::JsonSubset },
            ApiRejectionId::JsonStringifyF16 => WitnessEntry::Reachable { files: &["a043.ts", "a043-b.ts"], variant: Divergence::JsonSubset },
            ApiRejectionId::JsonParseDateText => WitnessEntry::Reachable { files: &["a045.ts", "a045-b.ts"], variant: Divergence::JsonSubset },
            ApiRejectionId::FormIsNaNValue => {
                WitnessEntry::Reachable { files: &["a046.ts", "a046-b.ts"], variant: Divergence::NumberCoercionAndArguments }
            }
            ApiRejectionId::FormIsFiniteValue => {
                WitnessEntry::Reachable { files: &["a047.ts", "a047-b.ts"], variant: Divergence::NumberCoercionAndArguments }
            }
            ApiRejectionId::FormParseIntValue => {
                WitnessEntry::Reachable { files: &["a048.ts", "a048-b.ts"], variant: Divergence::NumberCoercionAndArguments }
            }
            ApiRejectionId::FormNumberValue => {
                WitnessEntry::Reachable { files: &["a049.ts", "a049-b.ts"], variant: Divergence::NumberCoercionAndArguments }
            }
            ApiRejectionId::FormNewNumberValue => {
                WitnessEntry::Reachable { files: &["a050.ts", "a050-b.ts"], variant: Divergence::NumberCoercionAndArguments }
            }
            ApiRejectionId::FormToLocaleString => {
                WitnessEntry::Reachable { files: &["a051.ts", "a051-b.ts"], variant: Divergence::LocaleNumberFormatting }
            }
            ApiRejectionId::FormToString => {
                WitnessEntry::Reachable { files: &["a052.ts", "a052-b.ts"], variant: Divergence::NumberCoercionAndArguments }
            }
            ApiRejectionId::FormToPrecision => {
                WitnessEntry::Reachable { files: &["a053.ts", "a053-b.ts"], variant: Divergence::NumberCoercionAndArguments }
            }
            ApiRejectionId::FormToFixedToStringToExponentialToPrecision => {
                WitnessEntry::Reachable { files: &["a054.ts", "a054-b.ts"], variant: Divergence::MethodTypeDomain }
            }
            ApiRejectionId::FormMaxMinHypotWithMoreThanTwoArguments => {
                WitnessEntry::Reachable { files: &["a055.ts", "a055-b.ts"], variant: Divergence::MathSubset }
            }
            ApiRejectionId::FormMathUsedAsAValue => WitnessEntry::Reachable { files: &["a056.ts", "a056-b.ts"], variant: Divergence::MathSubset },
            ApiRejectionId::FormDateParse => WitnessEntry::Reachable { files: &["a057.ts", "a057-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::FormNewDate => WitnessEntry::Reachable { files: &["a058.ts", "a058-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::FormNewDateYearMonth => WitnessEntry::Reachable { files: &["a059.ts", "a059-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::FormTemplateInterpolation => WitnessEntry::Reachable { files: &["a060.ts", "a060-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::FormDirectComparison => WitnessEntry::Reachable { files: &["a061.ts", "a061-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::FormSet => WitnessEntry::Reachable { files: &["a062.ts", "a062-b.ts"], variant: Divergence::DateSubset },
            ApiRejectionId::FormSort => WitnessEntry::Reachable { files: &["a063.ts", "a063-b.ts"], variant: Divergence::ArrayMethodDefaults },
            ApiRejectionId::FormReduceCallback => {
                WitnessEntry::Reachable { files: &["a064.ts", "a064-b.ts"], variant: Divergence::ArrayMethodDefaults }
            }
            ApiRejectionId::FormReduceRightCallback => {
                WitnessEntry::Reachable { files: &["a065.ts", "a065-b.ts"], variant: Divergence::ArrayMethodDefaults }
            }
            ApiRejectionId::FormCallbackValueIndexArray => {
                WitnessEntry::Reachable { files: &["a066.ts", "a066-b.ts"], variant: Divergence::EscapingCapture }
            }
            ApiRejectionId::FormSpliceStartDeleteCountItems => {
                WitnessEntry::Reachable { files: &["a067.ts", "a067-b.ts"], variant: Divergence::VariadicArguments }
            }
            ApiRejectionId::FormUnshiftValueValues => {
                WitnessEntry::Reachable { files: &["a068.ts", "a068-b.ts"], variant: Divergence::VariadicArguments }
            }
            ApiRejectionId::FormNonCallbackTMethods => WitnessEntry::Reachable { files: &["a069.ts", "a069-accepted.ts", "a069-member-accepted.ts"], variant: Divergence::FixedArrayMethods },
            ApiRejectionId::MapScalarGet => WitnessEntry::Reachable { files: &["a070.ts", "a070-b.ts"], variant: Divergence::MapScalarGet },
            ApiRejectionId::MapNonNullableGet => WitnessEntry::Reachable { files: &["a071.ts", "a071-b.ts"], variant: Divergence::MapNonNullableGet },
            ApiRejectionId::FormNewMapIterable => WitnessEntry::Reachable { files: &["a072.ts", "a072-b.ts"], variant: Divergence::NoTupleType },
            ApiRejectionId::FormNewSetMap => WitnessEntry::Reachable { files: &["a073.ts", "a073-b.ts", "r198-set-source-map-old.ts"], variant: Divergence::NoTupleType },
            ApiRejectionId::FormNewSetGeneratorT => {
                WitnessEntry::Reachable { files: &["a074.ts", "a074-b.ts"], variant: Divergence::GeneratorSingleUse }
            }
            ApiRejectionId::FormArrayUsedAsAValue => {
                WitnessEntry::Reachable { files: &["a075.ts", "a075-b.ts"], variant: Divergence::CompilerOwnedValue }
            }
            ApiRejectionId::FormArrayFromSourceMapFn => {
                WitnessEntry::Reachable { files: &["a076.ts", "a076-b.ts"], variant: Divergence::ArrayFromMapper }
            }
            ApiRejectionId::FormArrayFromMap => WitnessEntry::Reachable { files: &["a077.ts", "a077-b.ts"], variant: Divergence::BareMapToArray },
            ApiRejectionId::FormArrayFromGeneratorT => {
                WitnessEntry::Reachable { files: &["a078.ts", "a078-b.ts"], variant: Divergence::GeneratorSingleUse }
            }
            ApiRejectionId::FormIsArrayValue => WitnessEntry::Reachable { files: &["a079.ts", "a079-b.ts"], variant: Divergence::ArrayIsArray },
            ApiRejectionId::FormOfValue => WitnessEntry::Reachable { files: &["a080.ts", "a080-b.ts"], variant: Divergence::ArrayOfArity },
            ApiRejectionId::FormNewArrayLength => {
                WitnessEntry::Reachable { files: &["a081.ts", "a081-b.ts"], variant: Divergence::ArrayHoleConstruction }
            }
            ApiRejectionId::FormGroupBy => WitnessEntry::Reachable { files: &["a082.ts"], variant: Divergence::DynamicObjectModel },
            ApiRejectionId::FormAlgebraNonSet => WitnessEntry::Reachable { files: &["a083.ts", "a083-old.ts"], variant: Divergence::SetAlgebraDomain },
        },
        RejectionSite::MirrorParameter => {
            WitnessEntry::Reachable { files: &["mirror-main.ts", "mirror-main-b.ts"], variant: Divergence::MirrorParameterPattern }
        }
        RejectionSite::ArraySpreadFixedArray => WitnessEntry::Reachable { files: &["s001.ts", "s001-b.ts"], variant: Divergence::FixedArraySpread },
        RejectionSite::ArraySpreadMap => WitnessEntry::Reachable { files: &["s002.ts", "s002-b.ts"], variant: Divergence::BareMapToArray },
        RejectionSite::ArraySpreadGenerator => WitnessEntry::Reachable { files: &["s003.ts", "s003-b.ts"], variant: Divergence::GeneratorSingleUse },
        RejectionSite::ArraySpreadSource => WitnessEntry::Reachable { files: &["s004.ts", "s004-old.ts"], variant: Divergence::UserIterationProtocol },
        RejectionSite::MapCopyKey => WitnessEntry::Reachable { files: &["s005.ts", "s005-c.ts"], variant: Divergence::MapKeyKind },
        RejectionSite::ContextBytesMissingType => {
            WitnessEntry::Reachable { files: &["s006.ts", "s006-c.ts"], variant: Divergence::ExplicitIntrinsicTypeArguments }
        }
        RejectionSite::ContextBytesTypeCount => WitnessEntry::Reachable { files: &["s007.ts"], variant: Divergence::ExplicitIntrinsicTypeArguments },
        RejectionSite::ContextBytesArgumentCount => WitnessEntry::Reachable { files: &["s008.ts"], variant: Divergence::ExplicitIntrinsicTypeArguments },
        RejectionSite::ContextBytesSpread => WitnessEntry::Reachable { files: &["s009.ts", "s009-b.ts"], variant: Divergence::VariadicArguments },
        RejectionSite::CallSpread => WitnessEntry::Reachable { files: &["s010.ts", "s010-b.ts", "r201-new-class-spread-variadic-old.ts", "r78-call-spread-variadic-old.ts"], variant: Divergence::VariadicArguments },
        RejectionSite::SetSourceSpread => WitnessEntry::Reachable { files: &["s011.ts", "s011-b.ts"], variant: Divergence::VariadicArguments },
        RejectionSite::SetSourceDomain => WitnessEntry::Reachable { files: &["s012.ts", "s012-c.ts"], variant: Divergence::SourceConstructionDomain },
        RejectionSite::NewMapSetKey => WitnessEntry::Reachable { files: &["s013.ts", "s013-c.ts"], variant: Divergence::MapKeyKind },
        RejectionSite::RegexSticky => WitnessEntry::Reachable { files: &["s014.ts", "s014-b.ts"], variant: Divergence::RegExpSubset },
        RejectionSite::ContextValue => WitnessEntry::Reachable { files: &["s015.ts", "s015-b.ts"], variant: Divergence::CompilerOwnedValue },
        RejectionSite::NumberValue => WitnessEntry::Reachable { files: &["s016.ts", "s016-b.ts"], variant: Divergence::CompilerOwnedValue },
        RejectionSite::JsonValue => WitnessEntry::Reachable { files: &["s017.ts", "s017-b.ts"], variant: Divergence::CompilerOwnedValue },
        RejectionSite::DateValue => WitnessEntry::Reachable { files: &["s018.ts", "s018-b.ts"], variant: Divergence::CompilerOwnedValue },
        RejectionSite::MapSetValue => WitnessEntry::Reachable { files: &["s019.ts", "s019-b.ts"], variant: Divergence::CompilerOwnedValue },
        RejectionSite::NumberGlobalValue => WitnessEntry::Reachable { files: &["s020.ts", "s020-b.ts"], variant: Divergence::CompilerOwnedValue },
        RejectionSite::CoercingGlobalValue => {
            WitnessEntry::Reachable { files: &["s021.ts", "s021-b.ts"], variant: Divergence::NumberCoercionAndArguments }
        }
        RejectionSite::RegexMember => WitnessEntry::Reachable { files: &["s022.ts", "s022-b.ts"], variant: Divergence::RegExpSubset },
        RejectionSite::DateMemberWrite => WitnessEntry::Reachable { files: &["s023.ts", "s023-b.ts"], variant: Divergence::DateSubset },
        RejectionSite::DateMethodValue => WitnessEntry::Reachable { files: &["s024.ts", "s024-b.ts"], variant: Divergence::CompilerOwnedValue },
        RejectionSite::NumberMethodValue => WitnessEntry::Reachable { files: &["s025.ts", "s025-b.ts"], variant: Divergence::CompilerOwnedValue },
        RejectionSite::NumberMethodArgumentCount => WitnessEntry::Reachable { files: &["s026.ts"], variant: Divergence::NumberCoercionAndArguments },
        RejectionSite::ArrayElementDomain => WitnessEntry::Reachable { files: &["s027.ts", "s027-b.ts"], variant: Divergence::MethodTypeDomain },
        RejectionSite::ArrayJoinDomain => WitnessEntry::Reachable { files: &["s028.ts", "s028-b.ts"], variant: Divergence::ArrayJoinDomain },
        RejectionSite::ArrayCallbackSpread => WitnessEntry::Reachable { files: &["s029.ts", "s029-b.ts"], variant: Divergence::VariadicArguments },
        RejectionSite::ArrayAccumulatorDomain => WitnessEntry::Reachable { files: &["s030.ts", "s030-b.ts"], variant: Divergence::MethodTypeDomain },
        RejectionSite::ArrayMapResult => WitnessEntry::Reachable { files: &["s031.ts", "s031-b.ts"], variant: Divergence::MethodTypeDomain },
        RejectionSite::MapGroupByKey => WitnessEntry::Reachable { files: &["s032.ts", "s032-b.ts"], variant: Divergence::MapKeyKind },
        RejectionSite::ArrayStaticMember => WitnessEntry::Reachable { files: &["s033.ts", "s033-c.ts"], variant: Divergence::NamespaceObjectMember },
        RejectionSite::ArrayFromTypeCount => {
            WitnessEntry::Reachable { files: &["s034.ts", "s034-d.ts"], variant: Divergence::ExplicitIntrinsicTypeArguments }
        }
        RejectionSite::ArrayFromArgumentCount => WitnessEntry::Reachable { files: &["s035.ts"], variant: Divergence::ArrayFromMapper },
        RejectionSite::ArrayFromSpread => WitnessEntry::Reachable { files: &["s036.ts", "s036-b.ts"], variant: Divergence::VariadicArguments },
        RejectionSite::ArrayFromSource => WitnessEntry::Reachable { files: &["s037.ts", "s037-b.ts"], variant: Divergence::SourceConstructionDomain },
        RejectionSite::CallbackParameterCount => {
            WitnessEntry::Reachable { files: &["s038.ts", "s038-d.ts"], variant: Divergence::CallbackParameterShape }
        }
        RejectionSite::ContextMember => WitnessEntry::Reachable { files: &["s039.ts", "s039-b.ts"], variant: Divergence::CompilerOwnedValue },
        RejectionSite::JsonMember => WitnessEntry::Reachable { files: &["s040.ts", "s040-b.ts"], variant: Divergence::CompilerOwnedValue },
        RejectionSite::ArrayFromValue => WitnessEntry::Reachable { files: &["s041.ts", "s041-b.ts"], variant: Divergence::CompilerOwnedValue },
        RejectionSite::ArrayMember => WitnessEntry::Reachable { files: &["s042.ts", "s042-b.ts"], variant: Divergence::NamespaceObjectMember },
        RejectionSite::MapGroupByValue => WitnessEntry::Reachable { files: &["s043.ts", "s043-b.ts"], variant: Divergence::CompilerOwnedValue },
        RejectionSite::MapSetMember => WitnessEntry::Reachable { files: &["s044.ts", "s044-b.ts"], variant: Divergence::NamespaceObjectMember },
        RejectionSite::MathMemberWrite => WitnessEntry::Reachable { files: &["s045.ts", "s045-b.ts"], variant: Divergence::MathSubset },
        RejectionSite::MathMethodValue => WitnessEntry::Reachable { files: &["s046.ts", "s046-b.ts"], variant: Divergence::MathSubset },
        RejectionSite::MathMember => WitnessEntry::Reachable { files: &["s047.ts", "s047-c.ts"], variant: Divergence::MathSubset },
        RejectionSite::MathCallCount => WitnessEntry::Reachable { files: &["s048.ts", "s048-b.ts"], variant: Divergence::MathSubset },
        RejectionSite::NumberMemberWrite => WitnessEntry::Reachable { files: &["s049.ts", "s049-b.ts"], variant: Divergence::CompilerOwnedValue },
        RejectionSite::NumberStaticValue => WitnessEntry::Reachable { files: &["s050.ts", "s050-b.ts"], variant: Divergence::CompilerOwnedValue },
        RejectionSite::NumberMember => WitnessEntry::Reachable { files: &["s051.ts", "s051-b.ts"], variant: Divergence::NamespaceObjectMember },
        RejectionSite::NumberPredicateCount => WitnessEntry::Reachable { files: &["s052.ts"], variant: Divergence::NumberCoercionAndArguments },
        RejectionSite::NumberGlobalCount => WitnessEntry::Reachable { files: &["s053.ts", "s053-old.ts"], variant: Divergence::NumberCoercionAndArguments },
        RejectionSite::RegexExec => WitnessEntry::Reachable { files: &["s054.ts", "s054-b.ts"], variant: Divergence::RegExpSubset },
        RejectionSite::StringPatternSpread => WitnessEntry::Reachable { files: &["s055.ts", "s055-b.ts"], variant: Divergence::VariadicArguments },
        RejectionSite::StringSearchPattern => WitnessEntry::Reachable { files: &["s056.ts", "s056-b.ts"], variant: Divergence::StringSearchPattern },
        RejectionSite::DateStaticWrite => WitnessEntry::Reachable { files: &["s057.ts", "s057-b.ts"], variant: Divergence::CompilerOwnedValue },
        RejectionSite::DateStaticValue => WitnessEntry::Reachable { files: &["s058.ts", "s058-c.ts"], variant: Divergence::CompilerOwnedValue },
        RejectionSite::DateNewSpread => WitnessEntry::Reachable { files: &["s059.ts", "s059-b.ts"], variant: Divergence::VariadicArguments },
        RejectionSite::DateMember => WitnessEntry::Reachable { files: &["s060.ts", "s060-b.ts"], variant: Divergence::DateSubset },
        RejectionSite::Float16Unary => WitnessEntry::Reachable { files: &["s061.ts", "s061-b.ts"], variant: Divergence::StorageOnlyFloat16 },
        RejectionSite::JsonStaticMember => WitnessEntry::Reachable { files: &["s062.ts", "s062-c.ts"], variant: Divergence::NamespaceObjectMember },
        RejectionSite::JsonStringifyCount => WitnessEntry::Reachable { files: &["s063.ts", "s063-b.ts"], variant: Divergence::JsonCallArguments },
        RejectionSite::JsonStringifySpread => WitnessEntry::Reachable { files: &["s064.ts", "s064-b.ts"], variant: Divergence::VariadicArguments },
        RejectionSite::JsonStringifyDomain => WitnessEntry::Reachable { files: &["s065.ts", "s065-b.ts"], variant: Divergence::JsonTypeDomain },
        RejectionSite::JsonStringifyHelper => WitnessEntry::Unreachable { files: &["s066.ts"], variant: Divergence::JsonTypeDomain, reason: "The json_serializable guard and collect_json_types graph closure admit only serializer helper types." },
        RejectionSite::JsonParseCount => WitnessEntry::Reachable { files: &["s067.ts", "s067-old.ts"], variant: Divergence::JsonCallArguments },
        RejectionSite::JsonParseSpread => WitnessEntry::Reachable { files: &["s068.ts", "s068-b.ts"], variant: Divergence::VariadicArguments },
        RejectionSite::JsonParseTypeCount => WitnessEntry::Reachable { files: &["s069.ts"], variant: Divergence::JsonSubset },
        RejectionSite::JsonParseTarget => WitnessEntry::Reachable { files: &["s070.ts", "s070-b.ts"], variant: Divergence::JsonSubset },
        RejectionSite::JsonParseDomain => WitnessEntry::Reachable { files: &["s071.ts", "s071-b.ts"], variant: Divergence::JsonTypeDomain },
        RejectionSite::JsonParseHelper => WitnessEntry::Unreachable { files: &["s072.ts"], variant: Divergence::JsonTypeDomain, reason: "The serializable, Error, and Date guards plus graph closure admit only parser helper types." },
        RejectionSite::JsonError => WitnessEntry::Reachable { files: &["s073.ts", "s073-b.ts"], variant: Divergence::JsonSubset },
        RejectionSite::ForOfEntries => WitnessEntry::Reachable { files: &["s074.ts", "s074-b.ts"], variant: Divergence::NoTupleType },
        RejectionSite::ForOfKeys => WitnessEntry::Reachable { files: &["s075.ts"], variant: Divergence::IteratorTemporary },
        RejectionSite::ForOfMap => WitnessEntry::Reachable { files: &["s076.ts", "s076-b.ts"], variant: Divergence::BareMapSubject },
        RejectionSite::ForOfUserClass => WitnessEntry::Reachable { files: &["s077.ts", "s077-old.ts", "s077-corpus-old.ts"], variant: Divergence::UserIterationProtocol },
        RejectionSite::ForOfSubject => WitnessEntry::Reachable { files: &["s078.ts", "s078-accepted.ts", "s078-r73-for-of-object-old.ts", "s078-r74-for-of-number-old.ts"], variant: Divergence::IterationSubjectDomain },
        RejectionSite::RegexMatchType => WitnessEntry::Reachable { files: &["s079.ts", "s079-b.ts"], variant: Divergence::RegExpSubset },
        RejectionSite::MapSetTypeKey => WitnessEntry::Reachable { files: &["s080.ts", "s080-b.ts"], variant: Divergence::MapKeyKind },
        RejectionSite::Float16Update => WitnessEntry::Reachable { files: &["s081.ts", "s081-c.ts"], variant: Divergence::StorageOnlyFloat16 },
        RejectionSite::Float16Binary => WitnessEntry::Reachable { files: &["s082.ts", "s082-b.ts"], variant: Divergence::StorageOnlyFloat16 },
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
