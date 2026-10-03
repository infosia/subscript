//! Exhaustive witness index; expected variants do not read the production map.

use crate::ambient::ApiRejectionId;
use crate::check::rejection::RejectionSite;
use crate::divergence::Divergence;

pub(super) fn witness_key(site: RejectionSite) -> (&'static str, Option<Divergence>) {
    match site {
        RejectionSite::Api(row) => match row.id {
            ApiRejectionId::StringLocaleCompare => {
                ("a001.ts", Some(Divergence::LocaleSensitiveString))
            }
            ApiRejectionId::StringToLocaleUpperCase => {
                ("a002.ts", Some(Divergence::LocaleSensitiveString))
            }
            ApiRejectionId::StringToLocaleLowerCase => {
                ("a003.ts", Some(Divergence::LocaleSensitiveString))
            }
            ApiRejectionId::StringNormalize => ("a004.ts", Some(Divergence::UnicodeNormalization)),
            ApiRejectionId::RegexStringMatch => ("a005.ts", Some(Divergence::MatchOptionalIndex)),
            ApiRejectionId::RegexStringMatchAll => ("a006.ts", Some(Divergence::RegExpSubset)),
            ApiRejectionId::ArrayFind => ("a010.ts", Some(Divergence::ArrayMethodDefaults)),
            ApiRejectionId::ArrayFindLast => ("a011.ts", Some(Divergence::ArrayMethodDefaults)),
            ApiRejectionId::ArrayFlat => ("a012.ts", Some(Divergence::ArrayFlattenDepth)),
            ApiRejectionId::ArrayFlatMap => ("a013.ts", Some(Divergence::MethodTypeDomain)),
            ApiRejectionId::ArrayEntries => ("a014.ts", Some(Divergence::NoTupleType)),
            ApiRejectionId::ArrayKeys => ("a015.ts", Some(Divergence::IteratorTemporary)),
            ApiRejectionId::ArrayValues => ("a016.ts", Some(Divergence::IteratorTemporary)),
            ApiRejectionId::DateLocalGetFullYear => ("a017.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::DateLocalGetMonth => ("a018.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::DateLocalGetDate => ("a019.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::DateLocalGetDay => ("a020.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::DateLocalGetHours => ("a021.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::DateLocalGetMinutes => ("a022.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::DateLocalGetSeconds => ("a023.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::DateLocalGetMilliseconds => ("a024.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::DateLocalGetTimezoneOffset => ("a025.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::DateLocalGetYear => ("a026.ts", None),
            ApiRejectionId::DateStringToString => ("a027.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::DateStringToDateString => ("a028.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::DateStringToTimeString => ("a029.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::DateStringToLocaleString => ("a030.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::DateStringToLocaleDateString => {
                ("a031.ts", Some(Divergence::DateSubset))
            }
            ApiRejectionId::DateStringToLocaleTimeString => {
                ("a032.ts", Some(Divergence::DateSubset))
            }
            ApiRejectionId::MapKeys => ("a033.ts", Some(Divergence::IteratorTemporary)),
            ApiRejectionId::MapValues => ("a034.ts", Some(Divergence::IteratorTemporary)),
            ApiRejectionId::MapEntries => ("a035.ts", Some(Divergence::NoTupleType)),
            ApiRejectionId::SetKeys => ("a036.ts", Some(Divergence::IteratorTemporary)),
            ApiRejectionId::SetValues => ("a037.ts", Some(Divergence::IteratorTemporary)),
            ApiRejectionId::SetEntries => ("a038.ts", Some(Divergence::NoTupleType)),
            ApiRejectionId::JsonStringifyMapKV => ("a039.ts", Some(Divergence::JsonSubset)),
            ApiRejectionId::JsonStringifySetK => ("a040.ts", Some(Divergence::JsonSubset)),
            ApiRejectionId::JsonStringifyObject => ("a041.ts", Some(Divergence::JsonSubset)),
            ApiRejectionId::JsonStringifyFunction => ("a042.ts", Some(Divergence::JsonSubset)),
            ApiRejectionId::JsonStringifyF16 => ("a043.ts", Some(Divergence::JsonSubset)),
            ApiRejectionId::JsonParseDateText => ("a045.ts", Some(Divergence::JsonSubset)),
            ApiRejectionId::FormIsNaNValue => {
                ("a046.ts", Some(Divergence::NumberCoercionAndArguments))
            }
            ApiRejectionId::FormIsFiniteValue => {
                ("a047.ts", Some(Divergence::NumberCoercionAndArguments))
            }
            ApiRejectionId::FormParseIntValue => {
                ("a048.ts", Some(Divergence::NumberCoercionAndArguments))
            }
            ApiRejectionId::FormNumberValue => {
                ("a049.ts", Some(Divergence::NumberCoercionAndArguments))
            }
            ApiRejectionId::FormNewNumberValue => {
                ("a050.ts", Some(Divergence::NumberCoercionAndArguments))
            }
            ApiRejectionId::FormToLocaleString => {
                ("a051.ts", Some(Divergence::LocaleNumberFormatting))
            }
            ApiRejectionId::FormToString => {
                ("a052.ts", Some(Divergence::NumberCoercionAndArguments))
            }
            ApiRejectionId::FormToPrecision => {
                ("a053.ts", Some(Divergence::NumberCoercionAndArguments))
            }
            ApiRejectionId::FormToFixedToStringToExponentialToPrecision => {
                ("a054.ts", Some(Divergence::MethodTypeDomain))
            }
            ApiRejectionId::FormMaxMinHypotWithMoreThanTwoArguments => {
                ("a055.ts", Some(Divergence::MathSubset))
            }
            ApiRejectionId::FormMathUsedAsAValue => ("a056.ts", Some(Divergence::MathSubset)),
            ApiRejectionId::FormDateParse => ("a057.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::FormNewDate => ("a058.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::FormNewDateYearMonth => ("a059.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::FormTemplateInterpolation => ("a060.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::FormDirectComparison => ("a061.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::FormSet => ("a062.ts", Some(Divergence::DateSubset)),
            ApiRejectionId::FormSort => ("a063.ts", Some(Divergence::ArrayMethodDefaults)),
            ApiRejectionId::FormReduceCallback => {
                ("a064.ts", Some(Divergence::ArrayMethodDefaults))
            }
            ApiRejectionId::FormReduceRightCallback => {
                ("a065.ts", Some(Divergence::ArrayMethodDefaults))
            }
            ApiRejectionId::FormCallbackValueIndexArray => {
                ("a066.ts", Some(Divergence::EscapingCapture))
            }
            ApiRejectionId::FormSpliceStartDeleteCountItems => {
                ("a067.ts", Some(Divergence::VariadicArguments))
            }
            ApiRejectionId::FormUnshiftValueValues => {
                ("a068.ts", Some(Divergence::VariadicArguments))
            }
            ApiRejectionId::FormNonCallbackTMethods => ("a069.ts", None),
            ApiRejectionId::MapScalarGet => ("a070.ts", Some(Divergence::MapScalarGet)),
            ApiRejectionId::MapNonNullableGet => ("a071.ts", Some(Divergence::MapNonNullableGet)),
            ApiRejectionId::FormNewMapIterable => ("a072.ts", Some(Divergence::NoTupleType)),
            ApiRejectionId::FormNewSetMap => ("a073.ts", Some(Divergence::NoTupleType)),
            ApiRejectionId::FormNewSetGeneratorT => {
                ("a074.ts", Some(Divergence::GeneratorSingleUse))
            }
            ApiRejectionId::FormArrayUsedAsAValue => {
                ("a075.ts", Some(Divergence::CompilerOwnedValue))
            }
            ApiRejectionId::FormArrayFromSourceMapFn => {
                ("a076.ts", Some(Divergence::ArrayFromMapper))
            }
            ApiRejectionId::FormArrayFromMap => ("a077.ts", Some(Divergence::BareMapToArray)),
            ApiRejectionId::FormArrayFromGeneratorT => {
                ("a078.ts", Some(Divergence::GeneratorSingleUse))
            }
            ApiRejectionId::FormIsArrayValue => ("a079.ts", Some(Divergence::ArrayIsArray)),
            ApiRejectionId::FormOfValue => ("a080.ts", Some(Divergence::ArrayOfArity)),
            ApiRejectionId::FormNewArrayLength => {
                ("a081.ts", Some(Divergence::ArrayHoleConstruction))
            }
            ApiRejectionId::FormGroupBy => ("a082.ts", None),
            ApiRejectionId::FormAlgebraNonSet => ("a083.ts", None),
        },
        RejectionSite::MirrorParameter => {
            ("mirror-main.ts", Some(Divergence::MirrorParameterPattern))
        }
        RejectionSite::ArraySpreadFixedArray => ("s001.ts", Some(Divergence::FixedArraySpread)),
        RejectionSite::ArraySpreadMap => ("s002.ts", Some(Divergence::BareMapToArray)),
        RejectionSite::ArraySpreadGenerator => ("s003.ts", Some(Divergence::GeneratorSingleUse)),
        RejectionSite::ArraySpreadSource => ("s004.ts", None),
        RejectionSite::MapCopyKey => ("s005.ts", Some(Divergence::MapKeyKind)),
        RejectionSite::ContextBytesMissingType => {
            ("s006.ts", Some(Divergence::ExplicitIntrinsicTypeArguments))
        }
        RejectionSite::ContextBytesTypeCount => ("s007.ts", None),
        RejectionSite::ContextBytesArgumentCount => ("s008.ts", None),
        RejectionSite::ContextBytesSpread => ("s009.ts", Some(Divergence::VariadicArguments)),
        RejectionSite::CallSpread => ("s010.ts", Some(Divergence::VariadicArguments)),
        RejectionSite::SetSourceSpread => ("s011.ts", Some(Divergence::VariadicArguments)),
        RejectionSite::SetSourceDomain => ("s012.ts", Some(Divergence::SourceConstructionDomain)),
        RejectionSite::NewMapSetKey => ("s013.ts", Some(Divergence::MapKeyKind)),
        RejectionSite::RegexSticky => ("s014.ts", Some(Divergence::RegExpSubset)),
        RejectionSite::ContextValue => ("s015.ts", Some(Divergence::CompilerOwnedValue)),
        RejectionSite::NumberValue => ("s016.ts", Some(Divergence::CompilerOwnedValue)),
        RejectionSite::JsonValue => ("s017.ts", Some(Divergence::CompilerOwnedValue)),
        RejectionSite::DateValue => ("s018.ts", Some(Divergence::CompilerOwnedValue)),
        RejectionSite::MapSetValue => ("s019.ts", Some(Divergence::CompilerOwnedValue)),
        RejectionSite::NumberGlobalValue => ("s020.ts", Some(Divergence::CompilerOwnedValue)),
        RejectionSite::CoercingGlobalValue => {
            ("s021.ts", Some(Divergence::NumberCoercionAndArguments))
        }
        RejectionSite::RegexMember => ("s022.ts", Some(Divergence::RegExpSubset)),
        RejectionSite::DateMemberWrite => ("s023.ts", Some(Divergence::DateSubset)),
        RejectionSite::DateMethodValue => ("s024.ts", Some(Divergence::CompilerOwnedValue)),
        RejectionSite::NumberMethodValue => ("s025.ts", Some(Divergence::CompilerOwnedValue)),
        RejectionSite::NumberMethodArgumentCount => ("s026.ts", None),
        RejectionSite::ArrayElementDomain => ("s027.ts", Some(Divergence::MethodTypeDomain)),
        RejectionSite::ArrayJoinDomain => ("s028.ts", Some(Divergence::ArrayJoinDomain)),
        RejectionSite::ArrayCallbackSpread => ("s029.ts", Some(Divergence::VariadicArguments)),
        RejectionSite::ArrayAccumulatorDomain => ("s030.ts", Some(Divergence::MethodTypeDomain)),
        RejectionSite::ArrayMapResult => ("s031.ts", Some(Divergence::MethodTypeDomain)),
        RejectionSite::MapGroupByKey => ("s032.ts", Some(Divergence::MapKeyKind)),
        RejectionSite::ArrayStaticMember => ("s033.ts", Some(Divergence::NamespaceObjectMember)),
        RejectionSite::ArrayFromTypeCount => {
            ("s034.ts", Some(Divergence::ExplicitIntrinsicTypeArguments))
        }
        RejectionSite::ArrayFromArgumentCount => ("s035.ts", None),
        RejectionSite::ArrayFromSpread => ("s036.ts", Some(Divergence::VariadicArguments)),
        RejectionSite::ArrayFromSource => ("s037.ts", Some(Divergence::SourceConstructionDomain)),
        RejectionSite::CallbackParameterCount => {
            ("s038.ts", Some(Divergence::CallbackParameterShape))
        }
        RejectionSite::ContextMember => ("s039.ts", Some(Divergence::CompilerOwnedValue)),
        RejectionSite::JsonMember => ("s040.ts", Some(Divergence::CompilerOwnedValue)),
        RejectionSite::ArrayFromValue => ("s041.ts", Some(Divergence::CompilerOwnedValue)),
        RejectionSite::ArrayMember => ("s042.ts", Some(Divergence::NamespaceObjectMember)),
        RejectionSite::MapGroupByValue => ("s043.ts", Some(Divergence::CompilerOwnedValue)),
        RejectionSite::MapSetMember => ("s044.ts", Some(Divergence::NamespaceObjectMember)),
        RejectionSite::MathMemberWrite => ("s045.ts", Some(Divergence::MathSubset)),
        RejectionSite::MathMethodValue => ("s046.ts", Some(Divergence::MathSubset)),
        RejectionSite::MathMember => ("s047.ts", Some(Divergence::MathSubset)),
        RejectionSite::MathCallCount => ("s048.ts", Some(Divergence::MathSubset)),
        RejectionSite::NumberMemberWrite => ("s049.ts", Some(Divergence::CompilerOwnedValue)),
        RejectionSite::NumberStaticValue => ("s050.ts", Some(Divergence::CompilerOwnedValue)),
        RejectionSite::NumberMember => ("s051.ts", Some(Divergence::NamespaceObjectMember)),
        RejectionSite::NumberPredicateCount => ("s052.ts", None),
        RejectionSite::NumberGlobalCount => ("s053.ts", None),
        RejectionSite::RegexExec => ("s054.ts", Some(Divergence::RegExpSubset)),
        RejectionSite::StringPatternSpread => ("s055.ts", Some(Divergence::VariadicArguments)),
        RejectionSite::StringSearchPattern => ("s056.ts", Some(Divergence::StringSearchPattern)),
        RejectionSite::DateStaticWrite => ("s057.ts", Some(Divergence::CompilerOwnedValue)),
        RejectionSite::DateStaticValue => ("s058.ts", Some(Divergence::CompilerOwnedValue)),
        RejectionSite::DateNewSpread => ("s059.ts", Some(Divergence::VariadicArguments)),
        RejectionSite::DateMember => ("s060.ts", Some(Divergence::DateSubset)),
        RejectionSite::Float16Unary => ("s061.ts", Some(Divergence::StorageOnlyFloat16)),
        RejectionSite::JsonStaticMember => ("s062.ts", Some(Divergence::NamespaceObjectMember)),
        RejectionSite::JsonStringifyCount => ("s063.ts", Some(Divergence::JsonCallArguments)),
        RejectionSite::JsonStringifySpread => ("s064.ts", Some(Divergence::VariadicArguments)),
        RejectionSite::JsonStringifyDomain => ("s065.ts", Some(Divergence::JsonTypeDomain)),
        RejectionSite::JsonStringifyHelper => ("s066.ts", None),
        RejectionSite::JsonParseCount => ("s067.ts", None),
        RejectionSite::JsonParseSpread => ("s068.ts", Some(Divergence::VariadicArguments)),
        RejectionSite::JsonParseTypeCount => ("s069.ts", None),
        RejectionSite::JsonParseTarget => ("s070.ts", Some(Divergence::JsonSubset)),
        RejectionSite::JsonParseDomain => ("s071.ts", Some(Divergence::JsonTypeDomain)),
        RejectionSite::JsonParseHelper => ("s072.ts", None),
        RejectionSite::JsonError => ("s073.ts", Some(Divergence::JsonSubset)),
        RejectionSite::ForOfEntries => ("s074.ts", Some(Divergence::NoTupleType)),
        RejectionSite::ForOfKeys => ("s075.ts", None),
        RejectionSite::ForOfMap => ("s076.ts", Some(Divergence::BareMapSubject)),
        RejectionSite::ForOfUserClass => ("s077.ts", None),
        RejectionSite::ForOfSubject => ("s078.ts", None),
        RejectionSite::RegexMatchType => ("s079.ts", Some(Divergence::RegExpSubset)),
        RejectionSite::MapSetTypeKey => ("s080.ts", Some(Divergence::MapKeyKind)),
        RejectionSite::Float16Update => ("s081.ts", Some(Divergence::StorageOnlyFloat16)),
        RejectionSite::Float16Binary => ("s082.ts", Some(Divergence::StorageOnlyFloat16)),
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
];
