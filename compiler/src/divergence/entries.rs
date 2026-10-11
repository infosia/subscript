//! The complete divergence fragment table.

use super::{Divergence, DivergenceEntry};

impl Divergence {
    /// The four facts for this topic.
    ///
    /// This `match` is the whole table (§79 rule 1).
    #[must_use]
    pub fn entry(self) -> DivergenceEntry {
        match self {
            Divergence::FileModuleDisabled => DivergenceEntry {
                ts: "import {readFile} from \"node:fs/promises\"; export async function main(): Promise<void> { await readFile(\"text.txt\", \"utf8\"); }",
                subscript: "no equivalent; enable node:fs/promises in the build and provide the host file callbacks",
                why: "The host enables file access and supplies the I/O, so a build names each standard module that it enables.",
                collision: "C25",
            },
            Divergence::FileModuleEncoding => DivergenceEntry {
                ts: "import {readFile} from \"node:fs/promises\"; export async function main(): Promise<void> { const encoding = \"utf8\"; await readFile(\"text.txt\", encoding); }",
                subscript: "import {readFile} from \"node:fs/promises\"; export async function main(): Promise<void> { await readFile(\"text.txt\", \"utf8\"); }",
                why: "The checker has no literal string type, so only the literal \"utf8\" argument selects the text read.",
                collision: "C25",
            },
            Divergence::FileModuleNumberArray => DivergenceEntry {
                ts: "import {writeFile} from \"node:fs/promises\"; export async function main(): Promise<void> { const data: i32[] = [1, 2]; await writeFile(\"data.bin\", data); }",
                subscript: "import {writeFile} from \"node:fs/promises\"; export async function main(): Promise<void> { const data: u8[] = [1, 2]; await writeFile(\"data.bin\", data); }",
                why: "The prelude declares every number type as number, but the bytes write takes the u8[] layout only.",
                collision: "C25",
            },
            Divergence::FileModuleStringAlias => DivergenceEntry {
                ts: "import {readFile} from \"node:fs/promises\"; type Name = \"a.txt\" | \"b.txt\"; export async function main(): Promise<void> { const name: Name = \"a.txt\"; await readFile(name, \"utf8\"); }",
                subscript: "import {readFile} from \"node:fs/promises\"; export async function main(): Promise<void> { const name: string = \"a.txt\"; await readFile(name, \"utf8\"); }",
                why: "A string-literal union alias value is an integer tag at run time, so it has no string for the host.",
                collision: "C25",
            },
            Divergence::FileModuleImportForm => DivergenceEntry {
                ts: "import * as Files from \"node:fs/promises\"; export async function main(): Promise<void> { await Files.readFile(\"text.txt\", \"utf8\"); }",
                subscript: "import {readFile} from \"node:fs/promises\"; export async function main(): Promise<void> { await readFile(\"text.txt\", \"utf8\"); }",
                why: "The checker resolves each file call at its named import, so a namespace, default, bare import, or re-export has no form.",
                collision: "C25",
            },
            Divergence::FileModuleFunctionValue => DivergenceEntry {
                ts: "import {readFile} from \"node:fs/promises\"; export async function main(): Promise<void> { const read = readFile; await read(\"text.txt\", \"utf8\"); }",
                subscript: "import {readFile} from \"node:fs/promises\"; export async function main(): Promise<void> { await readFile(\"text.txt\", \"utf8\"); }",
                why: "Each file call resolves to one host operation by its arguments, so a file function has no value form.",
                collision: "C25",
            },
            Divergence::TextModuleImportForm => DivergenceEntry {
                ts: "import * as Text from \"subscript:text\"; export function main(): void { print(`${Text.graphemeLength(\"か\")}`); }",
                subscript: "import {graphemeLength} from \"subscript:text\"; export function main(): void { print(`${graphemeLength(\"か\")}`); }",
                why: "The checker resolves each text call at its named import, so a namespace, bare import, or re-export has no form.",
                collision: "C26",
            },
            Divergence::TextModuleFunctionValue => DivergenceEntry {
                ts: "import {graphemeLength} from \"subscript:text\"; export function main(): void { const count = graphemeLength; print(`${count(\"か\")}`); }",
                subscript: "import {graphemeLength} from \"subscript:text\"; export function main(): void { print(`${graphemeLength(\"か\")}`); }",
                why: "Each text call lowers to one runtime operation, so a text function has no value form.",
                collision: "C26",
            },
            Divergence::CaptureOutlivesBlock => DivergenceEntry {
                ts: "async function work(): Promise<i32> { return 7; } export function main(): void { let f: () => Promise<i32> = work; { const h = work(); f = () => h; } }",
                subscript: "async function work(): Promise<i32> { return 7; } export function main(): void { { const h = work(); let f: () => Promise<i32> = () => h; } }",
                why: "A lambda borrows a captured counted binding. Its block exit releases that binding, so a local outside the block cannot receive the lambda.",
                collision: "C5",
            },
            Divergence::ArrayLengthStore => DivergenceEntry {
                ts: "export function main(): void { const xs: i32[] = [1]; xs.length = 2; xs.length -= 1; const n = (xs.length = 0); }",
                subscript: "export function main(): void { const xs: i32[] = [1]; xs.length = 0; xs.push(2); xs.pop(); }",
                why: "A nonzero length store can grow an array with holes. The language has no hole value. Only a literal zero store statement clears it.",
                collision: "stdlib.md §9.12",
            },
            Divergence::ErrorMemberOutsideSurface => DivergenceEntry {
                ts: "export function main(): void { const e = new Error(\"x\"); print(`${e.stack}`); print(`${e.cause}`); }",
                subscript: "export function main(): void { const e = new Error(\"x\"); print(e.message); }",
                why: "The runtime has no call-stack representation. Error cause needs an arbitrary payload and an absent value; the language has no undefined.",
                collision: "stdlib.md §19",
            },
            Divergence::NestedNominalClass => DivergenceEntry {
                ts: "class P { x: i32 = 1; } class Q { x: i32 = 1; } function q(): Q { return new Q(); } export function main(): void { const qs: Q[] = [new Q()]; const ps: P[] = qs; const f: () => P = q; }",
                subscript: "class P { x: i32 = 1; } function p(): P { return new P(); } export function main(): void { const ps: P[] = [new P()]; const f: () => P = p; }",
                why: "Class identity remains nominal inside arrays and function types, even when TypeScript relates the classes structurally.",
                collision: "C1",
            },
            Divergence::AbstractMember => DivergenceEntry {
                ts: "abstract class P { abstract x: i32; } export function main(): void {}",
                subscript: "class P { x: i32 = 1; } export function main(): void {}",
                why: "No subclass can implement an abstract member because the language rejects class inheritance.",
                collision: "C24",
            },
            Divergence::TaskGroupPosition => super::DivergenceEntry {
                ts: "class Holder { group: TaskGroup; constructor(group: TaskGroup) { this.group = group; } } export function main(): void {}",
                subscript: "export async function main(): Promise<void> { const group: TaskGroup = new TaskGroup(); await group.join(); }",
                why: "A group needs a lexical release: array counts miss removals, fields end only at collection, and dropped generators have no scope exit.",
                collision: "C24",
            },
            Divergence::TaskGroupUnjoined => super::DivergenceEntry {
                ts: "export function main(): void { const group: TaskGroup = new TaskGroup(); }",
                subscript: "export async function main(): Promise<void> { const group: TaskGroup = new TaskGroup(); await group.join(); }",
                why: "A group requires a join in its declaring scope so every task result has an observer.",
                collision: "C8",
            },
            Divergence::PromiseAllVoidValue => super::promise_all::PROMISEALLVOIDVALUE,
            Divergence::PromiseAllTypeArguments => super::promise_all::PROMISEALLTYPEARGUMENTS,
            Divergence::PromiseAllInput => super::promise_all::PROMISEALLINPUT,
            Divergence::PromiseAllCountedResult => super::promise_all::PROMISEALLCOUNTEDRESULT,
            Divergence::PromiseReactionResult => super::promise_reaction::PROMISEREACTIONRESULT,
            Divergence::PromiseReactionParameter => {
                super::promise_reaction::PROMISEREACTIONPARAMETER
            }
            Divergence::PromiseVoidReactionParameter => {
                super::promise_reaction::PROMISEVOIDREACTIONPARAMETER
            }

            Divergence::FunctionValueOptionalArguments => {
                super::type_flow::FUNCTION_VALUE_OPTIONAL_ARGUMENTS
            }
            Divergence::IndexedReadNullCheck => expression_forms::INDEXEDREADNULLCHECK,
            Divergence::DynamicImportCall => super::type_flow::DYNAMICIMPORTCALL,
            Divergence::AbstractMethodBodyMissing => super::type_flow::ABSTRACTMETHODBODYMISSING,
            Divergence::ThisStaticMethodMember => super::type_flow::THISSTATICMETHODMEMBER,
            Divergence::ErasedAssignableEquality => super::type_flow::ERASEDASSIGNABLEEQUALITY,
            Divergence::DeclaredFieldWithoutValue => super::type_flow::DECLAREDFIELDWITHOUTVALUE,
            Divergence::ErasedAssignableTypeMismatch => {
                super::type_flow::ERASEDASSIGNABLETYPEMISMATCH
            }
            Divergence::NonNullableNullEquality => super::type_flow::NONNULLABLENULLEQUALITY,
            Divergence::ImportAnnotation => expression_forms::IMPORTANNOTATION,
            Divergence::ConstAssertionExpression => surface_forms::CONSTASSERTIONEXPRESSION,
            Divergence::StringConcatArgumentCount => surface_forms::STRINGCONCATARGUMENTCOUNT,
            Divergence::ArrayConcatArgumentCount => surface_forms::ARRAYCONCATARGUMENTCOUNT,
            Divergence::GeneratorNextArgumentCount => surface_forms::GENERATORNEXTARGUMENTCOUNT,
            Divergence::ArrayPushArgumentCount => surface_forms::ARRAYPUSHARGUMENTCOUNT,
            Divergence::FunctionParameterIdentity => expression_forms::FUNCTIONPARAMETERIDENTITY,
            Divergence::ArrayToFixedArray => expression_forms::ARRAYTOFIXEDARRAY,
            Divergence::EnumToInteger => expression_forms::ENUMTOINTEGER,
            Divergence::LiteralAliasToString => expression_forms::LITERALALIASTOSTRING,
            Divergence::LiteralAliasMethod => expression_forms::LITERALALIASMETHOD,
            Divergence::EnumMethod => expression_forms::ENUMMETHOD,
            Divergence::FunctionMethod => expression_forms::FUNCTIONMETHOD,
            Divergence::BooleanMethod => expression_forms::BOOLEANMETHOD,
            Divergence::LiteralAliasMember => expression_forms::LITERALALIASMEMBER,
            Divergence::EnumMember => expression_forms::ENUMMEMBER,
            Divergence::GeneratorMember => expression_forms::GENERATORMEMBER,
            Divergence::FunctionMember => expression_forms::FUNCTIONMEMBER,
            Divergence::BooleanMember => expression_forms::BOOLEANMEMBER,
            Divergence::MetaPropertyExpression => expression_forms::METAPROPERTYEXPRESSION,
            Divergence::ClassExpression => expression_forms::CLASSEXPRESSION,
            Divergence::TaggedTemplateExpression => expression_forms::TAGGEDTEMPLATEEXPRESSION,
            Divergence::CommaExpression => expression_forms::COMMAEXPRESSION,
            Divergence::InstantiationExpression => expression_forms::INSTANTIATIONEXPRESSION,
            Divergence::SatisfiesExpression => expression_forms::SATISFIESEXPRESSION,
            Divergence::AngleAssertionExpression => expression_forms::ANGLEASSERTIONEXPRESSION,
            Divergence::StringAliasAssertion => expression_forms::STRINGALIASASSERTION,
            Divergence::NullableClassAssertion => expression_forms::NULLABLECLASSASSERTION,
            Divergence::IntegerEnumAssertion => expression_forms::INTEGERENUMASSERTION,
            Divergence::IdentityAssertion => expression_forms::IDENTITYASSERTION,
            Divergence::BooleanRelationalOperand => expression_forms::BOOLEANRELATIONALOPERAND,
            Divergence::BinaryStringOperand => expression_forms::BINARYSTRINGOPERAND,
            Divergence::StringRelationalOperand => expression_forms::STRINGRELATIONALOPERAND,
            Divergence::BinaryEnumOperand => expression_forms::BINARYENUMOPERAND,
            Divergence::CompoundStringOperand => expression_forms::COMPOUNDSTRINGOPERAND,
            Divergence::CompoundEnumOperand => expression_forms::COMPOUNDENUMOPERAND,
            Divergence::ExponentOperator => expression_forms::EXPONENTOPERATOR,
            Divergence::InOperator => expression_forms::INOPERATOR,
            Divergence::UnaryPlusOperator => expression_forms::UNARYPLUSOPERATOR,
            Divergence::VoidOperator => expression_forms::VOIDOPERATOR,
            Divergence::TypeofOperator => expression_forms::TYPEOFOPERATOR,
            Divergence::FunctionTypeObjectPattern => expression_forms::FUNCTIONTYPEOBJECTPATTERN,
            Divergence::FunctionTypeArrayPattern => expression_forms::FUNCTIONTYPEARRAYPATTERN,
            Divergence::FunctionTypeRestParameter => expression_forms::FUNCTIONTYPERESTPARAMETER,
            Divergence::BigIntAnnotation => expression_forms::BIGINTANNOTATION,
            Divergence::SymbolAnnotation => expression_forms::SYMBOLANNOTATION,
            Divergence::UnknownAnnotation => expression_forms::UNKNOWNANNOTATION,
            Divergence::NeverAnnotation => expression_forms::NEVERANNOTATION,
            Divergence::TemplateLiteralAnnotation => expression_forms::TEMPLATELITERALANNOTATION,
            Divergence::BigIntLiteralAnnotation => expression_forms::BIGINTLITERALANNOTATION,
            Divergence::BooleanLiteralAnnotation => expression_forms::BOOLEANLITERALANNOTATION,
            Divergence::NumberLiteralAnnotation => expression_forms::NUMBERLITERALANNOTATION,
            Divergence::StringLiteralAnnotation => expression_forms::STRINGLITERALANNOTATION,
            Divergence::PredicateAnnotation => expression_forms::PREDICATEANNOTATION,
            Divergence::MappedAnnotation => expression_forms::MAPPEDANNOTATION,
            Divergence::IndexedAnnotation => expression_forms::INDEXEDANNOTATION,
            Divergence::OperatorAnnotation => expression_forms::OPERATORANNOTATION,
            Divergence::ConditionalAnnotation => expression_forms::CONDITIONALANNOTATION,
            Divergence::StructuralAnnotation => expression_forms::STRUCTURALANNOTATION,
            Divergence::QueryAnnotation => expression_forms::QUERYANNOTATION,
            Divergence::ThisAnnotation => expression_forms::THISANNOTATION,
            Divergence::TupleAnnotation => expression_forms::TUPLEANNOTATION,
            Divergence::AutoAccessorDeclaration => expression_forms::AUTOACCESSORDECLARATION,
            Divergence::StaticBlockDeclaration => expression_forms::STATICBLOCKDECLARATION,
            Divergence::PrivateMethodDeclaration => expression_forms::PRIVATEMETHODDECLARATION,
            Divergence::PrivateFieldDeclaration => expression_forms::PRIVATEFIELDDECLARATION,
            Divergence::DebuggerStatement => expression_forms::DEBUGGERSTATEMENT,
            Divergence::LabeledStatement => expression_forms::LABELEDSTATEMENT,
            Divergence::ForInStatement => expression_forms::FORINSTATEMENT,
            Divergence::DoWhileStatement => expression_forms::DOWHILESTATEMENT,
            Divergence::SourceNamespaceDeclaration => expression_forms::SOURCENAMESPACEDECLARATION,
            Divergence::SourceInterfaceDeclaration => expression_forms::SOURCEINTERFACEDECLARATION,
            Divergence::LocalInterfaceDeclaration => expression_forms::LOCALINTERFACEDECLARATION,
            Divergence::LocalAliasDeclaration => expression_forms::LOCALALIASDECLARATION,
            Divergence::LocalEnumDeclaration => expression_forms::LOCALENUMDECLARATION,
            Divergence::LocalFunctionDeclaration => expression_forms::LOCALFUNCTIONDECLARATION,
            Divergence::LocalClassDeclaration => expression_forms::LOCALCLASSDECLARATION,
            Divergence::RestParameter => super::type_flow::RESTPARAMETER,
            Divergence::TypeParameterDefault => surface_forms::TYPEPARAMETERDEFAULT,
            Divergence::CollectionCallbackTypeMismatchForm => {
                surface_forms::COLLECTIONCALLBACKTYPEMISMATCHFORM
            }
            Divergence::PrivateMemberAssignmentForm => surface_forms::PRIVATEMEMBERASSIGNMENTFORM,
            Divergence::NonPlaceAssignmentTargetForm => surface_forms::NONPLACEASSIGNMENTTARGETFORM,
            Divergence::LogicalOrPowerAssignmentForm => surface_forms::LOGICALORPOWERASSIGNMENTFORM,
            Divergence::FunctionExpressionForm => surface_forms::FUNCTIONEXPRESSIONFORM,
            Divergence::NonNullAssertionExpressionForm => {
                surface_forms::NONNULLASSERTIONEXPRESSIONFORM
            }
            Divergence::ThisInParameterDefaultArrow => surface_forms::THISINPARAMETERDEFAULTARROW,
            Divergence::ThisInValueTypeArrow => surface_forms::THISINVALUETYPEARROW,
            Divergence::EnumObjectMember => surface_forms::ENUMOBJECTMEMBER,
            Divergence::StaticMethodValueForm => surface_forms::STATICMETHODVALUEFORM,
            Divergence::ConstructorNotNamedClassForm => surface_forms::CONSTRUCTORNOTNAMEDCLASSFORM,
            Divergence::CoroutineReturnOrThrowCallForm => {
                surface_forms::COROUTINERETURNORTHROWCALLFORM
            }
            Divergence::GeneratorYieldTypeNotKnownForm => {
                surface_forms::GENERATORYIELDTYPENOTKNOWNFORM
            }
            Divergence::LibGlobalCall => surface_forms::LIBGLOBALCALL,
            Divergence::DescriptorLiteralAccessorForm => {
                surface_forms::DESCRIPTORLITERALACCESSORFORM
            }
            Divergence::DescriptorLiteralQuotedKeyForm => {
                surface_forms::DESCRIPTORLITERALQUOTEDKEYFORM
            }
            Divergence::DescriptorLiteralSpreadForm => surface_forms::DESCRIPTORLITERALSPREADFORM,
            Divergence::EmptyArrayInferenceForm => surface_forms::EMPTYARRAYINFERENCEFORM,
            Divergence::LibGlobalValue => surface_forms::LIBGLOBALVALUE,
            Divergence::AmbientFunctionValueForm => surface_forms::AMBIENTFUNCTIONVALUEFORM,
            Divergence::ForeignFunctionValueForm => surface_forms::FOREIGNFUNCTIONVALUEFORM,
            Divergence::EnumObjectValueForm => surface_forms::ENUMOBJECTVALUEFORM,
            Divergence::GenericFunctionValue => surface_forms::GENERICFUNCTIONVALUE,
            Divergence::TemplateInterpolationKindForm => {
                surface_forms::TEMPLATEINTERPOLATIONKINDFORM
            }
            Divergence::BigIntLiteral => surface_forms::BIGINTLITERAL,
            Divergence::LambdaReturnFlowCoverage => surface_forms::LAMBDARETURNFLOWCOVERAGE,
            Divergence::GeneratorResultValueWriteForm => {
                surface_forms::GENERATORRESULTVALUEWRITEFORM
            }
            Divergence::GeneratorResultDoneWriteForm => surface_forms::GENERATORRESULTDONEWRITEFORM,
            Divergence::StringMethodValueForm => surface_forms::STRINGMETHODVALUEFORM,
            Divergence::SetMethodValueForm => surface_forms::SETMETHODVALUEFORM,
            Divergence::MapMethodValueForm => surface_forms::MAPMETHODVALUEFORM,
            Divergence::FixedArrayMethodValueForm => surface_forms::FIXEDARRAYMETHODVALUEFORM,
            Divergence::ArrayMethodValueForm => surface_forms::ARRAYMETHODVALUEFORM,
            Divergence::NonIndexableReceiverForm => surface_forms::NONINDEXABLERECEIVERFORM,
            Divergence::FixedArrayConstantIndexBoundsForm => {
                surface_forms::FIXEDARRAYCONSTANTINDEXBOUNDSFORM
            }
            Divergence::FixedArrayIndexNotIntForm => surface_forms::FIXEDARRAYINDEXNOTINTFORM,
            Divergence::ArrayIndexNotIntForm => surface_forms::ARRAYINDEXNOTINTFORM,
            Divergence::PrivateMemberReadForm => surface_forms::PRIVATEMEMBERREADFORM,
            Divergence::YieldDelegationForm => surface_forms::YIELDDELEGATIONFORM,
            Divergence::ConditionalNonBooleanConditionForm => {
                surface_forms::CONDITIONALNONBOOLEANCONDITIONFORM
            }
            Divergence::LogicalNonBooleanOperandForm => surface_forms::LOGICALNONBOOLEANOPERANDFORM,
            Divergence::LogicalNotNonBooleanForm => surface_forms::LOGICALNOTNONBOOLEANFORM,
            Divergence::DistinctNominalContainerAssignmentForm => {
                surface_forms::DISTINCTNOMINALCONTAINERASSIGNMENTFORM
            }
            Divergence::ConstructorTypeAnnotationForm => {
                surface_forms::CONSTRUCTORTYPEANNOTATIONFORM
            }
            Divergence::IntersectionTypeAnnotationForm => {
                surface_forms::INTERSECTIONTYPEANNOTATIONFORM
            }
            Divergence::LibTypeName => surface_forms::LIBTYPENAME,
            Divergence::GeneratorYieldTypeMissingForm => {
                surface_forms::GENERATORYIELDTYPEMISSINGFORM
            }
            Divergence::ArrayTypeArgument => surface_forms::ARRAYTYPEARGUMENT,
            Divergence::QualifiedSourceTypeNameForm => surface_forms::QUALIFIEDSOURCETYPENAMEFORM,
            Divergence::GenericClassDefaultParameterAnnotationNeeded => {
                surface_forms::GENERICCLASSDEFAULTPARAMETERANNOTATIONNEEDED
            }
            Divergence::BlockLambdaReturnAnnotationMissingForm => {
                surface_forms::BLOCKLAMBDARETURNANNOTATIONMISSINGFORM
            }
            Divergence::GenericCallbackParameterAnnotationNeeded => {
                surface_forms::GENERICCALLBACKPARAMETERANNOTATIONNEEDED
            }
            Divergence::FunctionValueParameterAnnotationNeeded => {
                surface_forms::FUNCTIONVALUEPARAMETERANNOTATIONNEEDED
            }
            Divergence::FunctionReturnAnnotationMissingForm => {
                surface_forms::FUNCTIONRETURNANNOTATIONMISSINGFORM
            }
            Divergence::NamedImportModuleMissingForm => surface_forms::NAMEDIMPORTMODULEMISSINGFORM,
            Divergence::NamespaceImportTargetMissingForm => {
                surface_forms::NAMESPACEIMPORTTARGETMISSINGFORM
            }
            Divergence::MirrorModuleDeclarationForm => surface_forms::MIRRORMODULEDECLARATIONFORM,
            Divergence::DuplicateLiteralAliasMemberForm => {
                surface_forms::DUPLICATELITERALALIASMEMBERFORM
            }
            Divergence::SourceAliasNotLiteralUnionForm => {
                surface_forms::SOURCEALIASNOTLITERALUNIONFORM
            }
            Divergence::GenericSourceAliasForm => surface_forms::GENERICSOURCEALIASFORM,
            Divergence::EnumStringMemberNameForm => surface_forms::ENUMSTRINGMEMBERNAMEFORM,
            Divergence::ConstructorParameterPropertyForm => {
                surface_forms::CONSTRUCTORPARAMETERPROPERTYFORM
            }
            Divergence::FieldTypeWithoutInitializer => surface_forms::FIELDTYPEWITHOUTINITIALIZER,
            Divergence::IdentifierFieldName => surface_forms::IDENTIFIERFIELDNAME,
            Divergence::GeneratorMethodForm => surface_forms::GENERATORMETHODFORM,
            Divergence::SwitchDiscriminantKindForm => surface_forms::SWITCHDISCRIMINANTKINDFORM,
            Divergence::ForOfSpreadCall => surface_forms::FOROFSPREADCALL,
            Divergence::ForOfBindingKindForm => surface_forms::FOROFBINDINGKINDFORM,
            Divergence::ForOfVarBindingForm => surface_forms::FOROFVARBINDINGFORM,
            Divergence::AsyncForOfForm => surface_forms::ASYNCFOROFFORM,
            Divergence::StatementNonBooleanConditionForm => {
                surface_forms::STATEMENTNONBOOLEANCONDITIONFORM
            }
            Divergence::GeneratorReturnValue => surface_forms::GENERATORRETURNVALUE,
            Divergence::LocalTypeWithoutInitializerForm => {
                surface_forms::LOCALTYPEWITHOUTINITIALIZERFORM
            }
            Divergence::LocalVarDeclarationForm => surface_forms::LOCALVARDECLARATIONFORM,
            Divergence::ReturnFlowCoverage => surface_forms::RETURNFLOWCOVERAGE,
            Divergence::ModuleInitializerMissingForm => surface_forms::MODULEINITIALIZERMISSINGFORM,
            Divergence::ModuleVarDeclarationForm => surface_forms::MODULEVARDECLARATIONFORM,
            Divergence::LibConstructorName => surface_forms::LIBCONSTRUCTORNAME,
            Divergence::GeneratorFunctionValueForm => surface_forms::GENERATORFUNCTIONVALUEFORM,
            Divergence::GenericSynchronousMethodValueForm => {
                surface_forms::GENERICSYNCHRONOUSMETHODVALUEFORM
            }
            Divergence::SynchronousMethodValueForm => surface_forms::SYNCHRONOUSMETHODVALUEFORM,
            Divergence::MapCopyNullableSource => builtin_calls::MAP_COPY_NULLABLE_SOURCE,

            Divergence::ClassInheritedObjectMember => builtin_calls::CLASS_INHERITED_OBJECT_MEMBER,
            Divergence::ClassRuntimeObject => builtin_calls::CLASS_RUNTIME_OBJECT,

            Divergence::UnaryNumericCoercion => builtin_calls::UNARY_NUMERIC_COERCION,
            Divergence::BitwiseIntegerOperand => builtin_calls::BITWISE_INTEGER_OPERAND,
            Divergence::DeleteProperty => builtin_calls::DELETE_PROPERTY,
            Divergence::OptionalMethodCall => builtin_calls::OPTIONAL_METHOD_CALL,
            Divergence::OptionalFunctionCall => builtin_calls::OPTIONAL_FUNCTION_CALL,
            Divergence::UndefinedEqualityPair => builtin_calls::UNDEFINED_EQUALITY_PAIR,
            Divergence::UndefinedEqualityNonMember => builtin_calls::UNDEFINED_EQUALITY_NON_MEMBER,
            Divergence::BareYieldNonVoid => builtin_calls::BARE_YIELD_NON_VOID,
            Divergence::AsyncMethodValue => builtin_calls::ASYNC_METHOD_VALUE,
            Divergence::GenericAsyncMethodValue => builtin_calls::GENERIC_ASYNC_METHOD_VALUE,
            Divergence::FixedArrayObjectMember => builtin_calls::FIXED_ARRAY_OBJECT_MEMBER,
            Divergence::MapObjectMember => builtin_calls::MAP_OBJECT_MEMBER,
            Divergence::SetObjectMember => builtin_calls::SET_OBJECT_MEMBER,
            Divergence::GeneratorResultObjectMember => {
                builtin_calls::GENERATOR_RESULT_OBJECT_MEMBER
            }
            Divergence::NumericObjectMember => builtin_calls::NUMERIC_OBJECT_MEMBER,
            Divergence::BoundaryObjectMember => builtin_calls::BOUNDARY_OBJECT_MEMBER,
            Divergence::WorkerStaticObjectMethod => builtin_calls::WORKER_STATIC_OBJECT_METHOD,
            Divergence::WorkerObjectMethod => builtin_calls::WORKER_OBJECT_METHOD,
            Divergence::InboxObjectMethod => builtin_calls::INBOX_OBJECT_METHOD,
            Divergence::OutboxObjectMethod => builtin_calls::OUTBOX_OBJECT_METHOD,
            Divergence::FixedArrayObjectMethod => builtin_calls::FIXED_ARRAY_OBJECT_METHOD,
            Divergence::NumericObjectMethod => builtin_calls::NUMERIC_OBJECT_METHOD,
            Divergence::StringObjectMember => builtin_calls::STRING_OBJECT_MEMBER,
            Divergence::MapObjectMethod => builtin_calls::MAP_OBJECT_METHOD,
            Divergence::SetObjectMethod => builtin_calls::SET_OBJECT_METHOD,
            Divergence::ArrayObjectMember => builtin_calls::ARRAY_OBJECT_MEMBER,
            Divergence::RegexCompile => builtin_calls::REGEX_COMPILE,
            Divergence::FractionalIntegerLiteral => builtin_calls::FRACTIONAL_INTEGER_LITERAL,
            Divergence::FixedArrayLiteralLength => builtin_calls::FIXED_ARRAY_LITERAL_LENGTH,
            Divergence::ByteArgumentIdentity => builtin_calls::BYTE_ARGUMENT_IDENTITY,
            Divergence::GenericConstructorTypeArguments => {
                builtin_calls::GENERIC_CONSTRUCTOR_TYPE_ARGUMENTS
            }
            Divergence::WorkerSpawnSpread => builtin_calls::WORKER_SPAWN_SPREAD,
            Divergence::WorkerEntryLocalValue => builtin_calls::WORKER_ENTRY_LOCAL_VALUE,
            Divergence::WorkerEntryGeneric => builtin_calls::WORKER_ENTRY_GENERIC,
            Divergence::WorkerEntrySignature => builtin_calls::WORKER_ENTRY_SIGNATURE,
            Divergence::WorkerEntryStructuralEndpoints => {
                builtin_calls::WORKER_ENTRY_STRUCTURAL_ENDPOINTS
            }
            Divergence::WorkerExplicitMessageIdentity => {
                builtin_calls::WORKER_EXPLICIT_MESSAGE_IDENTITY
            }
            Divergence::AwaitNonHandle => builtin_calls::AWAIT_NON_HANDLE,
            Divergence::AwaitUndeclaredAsyncFunction => {
                builtin_calls::AWAIT_UNDECLARED_ASYNC_FUNCTION
            }
            Divergence::AwaitSynchronousFunction => builtin_calls::AWAIT_SYNCHRONOUS_FUNCTION,
            Divergence::AwaitComputedMethod => builtin_calls::AWAIT_COMPUTED_METHOD,
            Divergence::AwaitNonClassMethod => builtin_calls::AWAIT_NON_CLASS_METHOD,
            Divergence::AwaitSynchronousMethod => builtin_calls::AWAIT_SYNCHRONOUS_METHOD,
            Divergence::AwaitIndirectCall => builtin_calls::AWAIT_INDIRECT_CALL,
            Divergence::ArrayUnshiftEmpty => builtin_calls::ARRAY_UNSHIFT_EMPTY,
            Divergence::ArrayCallbackThisArgument => builtin_calls::ARRAY_CALLBACK_THIS_ARGUMENT,
            Divergence::MapCallbackThisArgument => builtin_calls::MAP_CALLBACK_THIS_ARGUMENT,
            Divergence::SetCallbackThisArgument => builtin_calls::SET_CALLBACK_THIS_ARGUMENT,
            Divergence::MapGroupByArraySource => builtin_calls::MAP_GROUP_BY_ARRAY_SOURCE,
            Divergence::MapGroupByVoidKey => builtin_calls::MAP_GROUP_BY_VOID_KEY,

            Divergence::NullInitializerInference => established::NULLINITIALIZERINFERENCE,
            Divergence::UsingBindingResourceType => established::USINGBINDINGRESOURCETYPE,
            Divergence::MirrorExportList => established::MIRROREXPORTLIST,
            Divergence::TopLevelNameClash => established::TOPLEVELNAMECLASH,
            Divergence::UnsupportedModuleDeclaration => established::UNSUPPORTEDMODULEDECLARATION,
            Divergence::PoisonedDefaultImport => established::POISONEDDEFAULTIMPORT,
            Divergence::DefaultImport => established::DEFAULTIMPORT,
            Divergence::ComputedMethodName => established::COMPUTEDMETHODNAME,
            Divergence::FunctionBodyMissing => established::FUNCTIONBODYMISSING,
            Divergence::StaticFieldInitializerMissing => established::STATICFIELDINITIALIZERMISSING,
            Divergence::FieldAssignmentAfterUnreachableReturn => {
                established::FIELDASSIGNMENTAFTERUNREACHABLERETURN
            }
            Divergence::FieldAssignmentMissingNoNormalExit => {
                established::FIELDASSIGNMENTMISSINGNONORMALEXIT
            }
            Divergence::ConstructorFieldReadWithAssignmentFact => {
                established::CONSTRUCTORDEFINITEFIELDREADBEFOREASSIGNMENT
            }
            Divergence::ForOfAwaitUsing => established::FOROFAWAITUSING,
            Divergence::AliasCaseNonLiteral => established::ALIASCASENONLITERAL,
            Divergence::ClassMemberNameClash => established::CLASSMEMBERNAMECLASH,
            Divergence::DescriptorMethod => established::DESCRIPTORMETHOD,
            Divergence::MirrorStaticMethod => established::MIRRORSTATICMETHOD,
            Divergence::DisposeStatic => established::DISPOSESTATIC,
            Divergence::MirrorAccessor => established::MIRRORACCESSOR,
            Divergence::ReadAccessorReturnMissing => established::READACCESSORRETURNMISSING,
            Divergence::WriteAccessorPattern => established::WRITEACCESSORPATTERN,
            Divergence::WriteAccessorTypeMissing => established::WRITEACCESSORTYPEMISSING,
            Divergence::GenericMethodBodyMissing => established::GENERICMETHODBODYMISSING,
            Divergence::DisposeAsync => established::DISPOSEASYNC,
            Divergence::DisposeSignature => established::DISPOSESIGNATURE,
            Divergence::DescriptorInheritance => established::DESCRIPTORINHERITANCE,
            Divergence::ReferenceClassInheritance => established::REFERENCECLASSINHERITANCE,
            Divergence::DescriptorStaticField => established::DESCRIPTORSTATICFIELD,
            Divergence::MirrorStaticField => established::MIRRORSTATICFIELD,
            Divergence::StaticFieldOptional => established::STATICFIELDOPTIONAL,
            Divergence::ContextAffineStaticField => established::CONTEXTAFFINESTATICFIELD,
            Divergence::DescriptorInitializerWithoutOptional => {
                established::DESCRIPTORINITIALIZERWITHOUTOPTIONAL
            }
            Divergence::DescriptorRequiredWithoutDefinite => {
                established::DESCRIPTORREQUIREDWITHOUTDEFINITE
            }
            Divergence::InstanceFieldOptional => established::INSTANCEFIELDOPTIONAL,
            Divergence::WireAliasNestedField => established::WIREALIASNESTEDFIELD,
            Divergence::ContextAffineInstanceField => established::CONTEXTAFFINEINSTANCEFIELD,
            Divergence::ValueFieldOutsideWhitelist => established::VALUEFIELDOUTSIDEWHITELIST,
            Divergence::DescriptorConstructor => established::DESCRIPTORCONSTRUCTOR,
            Divergence::WireAliasNestedConstructorParameter => {
                established::WIREALIASNESTEDCONSTRUCTORPARAMETER
            }
            Divergence::ClassIndexSignatureCount => established::CLASSINDEXSIGNATURECOUNT,
            Divergence::ClassIndexSignatureNonReference => {
                established::CLASSINDEXSIGNATURENONREFERENCE
            }
            Divergence::ClassIndexSignatureStatic => established::CLASSINDEXSIGNATURESTATIC,
            Divergence::ClassIndexSignatureIndexType => established::CLASSINDEXSIGNATUREINDEXTYPE,
            Divergence::WriteAccessorWithoutRead => established::WRITEACCESSORWITHOUTREAD,
            Divergence::AccessorTypeMismatch => established::ACCESSORTYPEMISMATCH,
            Divergence::ClassIndexSetSignature => established::CLASSINDEXSETSIGNATURE,
            Divergence::ModuleUsing => established::MODULEUSING,
            Divergence::DescriptorOptions => established::DESCRIPTOROPTIONS,
            Divergence::UnsupportedClassDecorator => established::UNSUPPORTEDCLASSDECORATOR,
            Divergence::DescriptorValueType => established::DESCRIPTORVALUETYPE,
            Divergence::EnumImplicitValueOverflow => established::ENUMIMPLICITVALUEOVERFLOW,
            Divergence::WireEnumEmpty => established::WIREENUMEMPTY,
            Divergence::WireEnumMemberForm => established::WIREENUMMEMBERFORM,
            Divergence::WireEnumMemberKey => established::WIREENUMMEMBERKEY,
            Divergence::MirrorVariableForm => established::MIRRORVARIABLEFORM,
            Divergence::WireAliasNestedForeignParameter => {
                established::WIREALIASNESTEDFOREIGNPARAMETER
            }
            Divergence::WireAliasNestedForeignReturn => established::WIREALIASNESTEDFOREIGNRETURN,
            Divergence::ForeignDirectCallback => established::FOREIGNDIRECTCALLBACK,
            Divergence::ForeignReturnProvenance => established::FOREIGNRETURNPROVENANCE,
            Divergence::ForeignResultRead => established::FOREIGNRESULTREAD,
            Divergence::ForeignStructCycle => established::FOREIGNSTRUCTCYCLE,
            Divergence::AsyncGeneratorFunction => established::ASYNCGENERATORFUNCTION,
            Divergence::AsyncReturnAnnotationMissing => established::ASYNCRETURNANNOTATIONMISSING,
            Divergence::OptionalParameter => established::OPTIONALPARAMETER,
            Divergence::ErrorMessageType => established::ERRORMESSAGETYPE,
            Divergence::ErrorConstructorArguments => established::ERRORCONSTRUCTORARGUMENTS,
            Divergence::ErrorCallWithoutNew => established::ERRORCALLWITHOUTNEW,
            Divergence::ValueTypeArgumentCount => established::VALUETYPEARGUMENTCOUNT,
            Divergence::ValueTypeOptionsNonLiteral => established::VALUETYPEOPTIONSNONLITERAL,
            Divergence::ValueTypeOptionCount => established::VALUETYPEOPTIONCOUNT,
            Divergence::ValueTypeOptionSpread => established::VALUETYPEOPTIONSPREAD,
            Divergence::ValueTypeOptionPropertyForm => established::VALUETYPEOPTIONPROPERTYFORM,
            Divergence::ValueTypeOptionKey => established::VALUETYPEOPTIONKEY,
            Divergence::ValueTypeAlignmentNonLiteral => established::VALUETYPEALIGNMENTNONLITERAL,
            Divergence::ValueTypeAlignmentOutsideSet => established::VALUETYPEALIGNMENTOUTSIDESET,
            Divergence::InvalidProgramEntry => established::INVALIDPROGRAMENTRY,
            Divergence::AsyncReturnNonReference => established::ASYNCRETURNNONREFERENCE,
            Divergence::AsyncReturnQualifiedName => established::ASYNCRETURNQUALIFIEDNAME,
            Divergence::AsyncReturnAlias => established::ASYNCRETURNALIAS,
            Divergence::AsyncReturnMissingArgument => established::ASYNCRETURNMISSINGARGUMENT,
            Divergence::AsyncReturnArgumentCount => established::ASYNCRETURNARGUMENTCOUNT,
            Divergence::FixedArrayLengthRange => established::FIXEDARRAYLENGTHRANGE,
            Divergence::FixedArrayLengthLiteral => established::FIXEDARRAYLENGTHLITERAL,
            Divergence::GenericConstraintIdentity => established::GENERICCONSTRAINTIDENTITY,
            Divergence::NamespaceUnexportedMember => established::NAMESPACEUNEXPORTEDMEMBER,
            Divergence::RunnerMainMissing => established::RUNNERMAINMISSING,

            Divergence::ClassFinalAlignmentLimit => established::CLASSFINALALIGNMENTLIMIT,
            Divergence::AggregateArgumentFrameLimit => established::AGGREGATEARGUMENTFRAMELIMIT,
            Divergence::NamespaceAsValue => established::NAMESPACEASVALUE,
            Divergence::SwitchCaseClosureRead => established::SWITCHCASEREAD,
            Divergence::BlockNameReadBeforeDeclaration => {
                established::BLOCKNAMEREADBEFOREDECLARATION
            }
            Divergence::BlockNameWriteBeforeDeclaration => {
                established::BLOCKNAMEWRITEBEFOREDECLARATION
            }
            Divergence::ContextAffineCapture => established::CONTEXTAFFINECAPTURE,
            Divergence::MutableLocalCapture => established::MUTABLELOCALCAPTURE,
            Divergence::ContextAffineArrayElement => established::CONTEXTAFFINEARRAYELEMENT,
            Divergence::ContextAffineContainerArgument => {
                established::CONTEXTAFFINECONTAINERARGUMENT
            }
            Divergence::WorkerMessagePlainClass => established::WORKERMESSAGEPLAINCLASS,
            Divergence::BoundaryLiteralAlias => established::BOUNDARYLITERALALIAS,
            Divergence::NullableNonReference => established::NULLABLENONREFERENCE,
            Divergence::NullableValueClassAssignment => established::NULLABLEVALUECLASSASSIGNMENT,
            Divergence::MirrorHeaderMissing => established::MIRRORHEADERMISSING,
            Divergence::MirrorParameterTargetMissing => established::MIRRORPARAMETERTARGETMISSING,
            Divergence::MirrorCallbackTargetMissing => established::MIRRORCALLBACKTARGETMISSING,
            Divergence::MirrorLifetimeTargetMissing => established::MIRRORLIFETIMETARGETMISSING,
            Divergence::MirrorArrayProvenanceMissing => established::MIRRORARRAYPROVENANCEMISSING,
            Divergence::MirrorStringProvenanceMissing => established::MIRRORSTRINGPROVENANCEMISSING,
            Divergence::MirrorParameterProvenanceMismatch => {
                established::MIRRORPARAMETERPROVENANCEMISMATCH
            }
            Divergence::MirrorAnonymousCallback => established::MIRRORANONYMOUSCALLBACK,
            Divergence::MirrorCallbackProvenanceMissing => {
                established::MIRRORCALLBACKPROVENANCEMISSING
            }
            Divergence::ProvenanceUnknownKind => established::PROVENANCEUNKNOWNKIND,
            Divergence::ProvenanceMissingKind => established::PROVENANCEMISSINGKIND,
            Divergence::ProvenanceFieldSeparator => established::PROVENANCEFIELDSEPARATOR,
            Divergence::ProvenanceUnexpectedKey => established::PROVENANCEUNEXPECTEDKEY,
            Divergence::ProvenanceUnquotedString => established::PROVENANCEUNQUOTEDSTRING,
            Divergence::ProvenanceUnterminatedString => established::PROVENANCEUNTERMINATEDSTRING,
            Divergence::ProvenanceUnterminatedEscape => established::PROVENANCEUNTERMINATEDESCAPE,
            Divergence::ProvenanceUnsupportedEscape => established::PROVENANCEUNSUPPORTEDESCAPE,
            Divergence::ProvenanceControlCharacter => established::PROVENANCECONTROLCHARACTER,
            Divergence::ProvenanceInvalidUnicodeDigits => {
                established::PROVENANCEINVALIDUNICODEDIGITS
            }
            Divergence::ProvenanceInvalidBoolean => established::PROVENANCEINVALIDBOOLEAN,
            Divergence::ProvenanceTrailingData => established::PROVENANCETRAILINGDATA,
            Divergence::ProvenanceShortUnicodeEscape => established::PROVENANCESHORTUNICODEESCAPE,
            Divergence::ProvenanceInvalidUnicodeScalar => {
                established::PROVENANCEINVALIDUNICODESCALAR
            }
            Divergence::ProvenanceHeaderBasename => established::PROVENANCEHEADERBASENAME,
            Divergence::ProvenanceDuplicateHeader => established::PROVENANCEDUPLICATEHEADER,
            Divergence::ProvenanceEmptyDescriptor => established::PROVENANCEEMPTYDESCRIPTOR,
            Divergence::ProvenanceDuplicateDescriptor => established::PROVENANCEDUPLICATEDESCRIPTOR,
            Divergence::ProvenanceEmptyStringView => established::PROVENANCEEMPTYSTRINGVIEW,
            Divergence::ProvenanceDuplicateStringView => established::PROVENANCEDUPLICATESTRINGVIEW,
            Divergence::ProvenanceEmptyScalarPair => established::PROVENANCEEMPTYSCALARPAIR,
            Divergence::ProvenanceDuplicateScalarPair => established::PROVENANCEDUPLICATESCALARPAIR,
            Divergence::ProvenanceEmptyCallback => established_tail::PROVENANCEEMPTYCALLBACK,
            Divergence::ProvenanceDuplicateCallback => {
                established_tail::PROVENANCEDUPLICATECALLBACK
            }
            Divergence::ProvenanceEmptyCallbackLifetime => {
                established_tail::PROVENANCEEMPTYCALLBACKLIFETIME
            }
            Divergence::ProvenanceDuplicateCallbackLifetime => {
                established_tail::PROVENANCEDUPLICATECALLBACKLIFETIME
            }
            Divergence::ProvenanceEmptyExternalType => {
                established_tail::PROVENANCEEMPTYEXTERNALTYPE
            }
            Divergence::ProvenanceDuplicateExternalType => {
                established_tail::PROVENANCEDUPLICATEEXTERNALTYPE
            }
            Divergence::ProvenanceEmptyCEnum => established_tail::PROVENANCEEMPTYCENUM,
            Divergence::ProvenanceDuplicateCEnum => established_tail::PROVENANCEDUPLICATECENUM,
            Divergence::ProvenanceEmptyMember => established_tail::PROVENANCEEMPTYMEMBER,
            Divergence::ProvenanceDuplicateMember => established_tail::PROVENANCEDUPLICATEMEMBER,
            Divergence::MirrorMemberTargetMissing => established_tail::MIRRORMEMBERTARGETMISSING,
            Divergence::ProvenanceEmptyPointerParameter => established_tail::PROVENANCEEMPTYPOINTERPARAMETER,
            Divergence::ProvenanceDuplicatePointerParameter => established_tail::PROVENANCEDUPLICATEPOINTERPARAMETER,

            Divergence::IterationSubjectDomain => established_tail::ITERATIONSUBJECTDOMAIN,
            Divergence::FixedArrayMethods => established_tail::FIXEDARRAYMETHODS,
            Divergence::CompilerOwnedValue => established_tail::COMPILEROWNEDVALUE,
            Divergence::NamespaceObjectMember => established_tail::NAMESPACEOBJECTMEMBER,
            Divergence::UnicodeNormalization => established_tail::UNICODENORMALIZATION,
            Divergence::MatchOptionalIndex => established_tail::MATCHOPTIONALINDEX,
            Divergence::ArrayFlattenDepth => established_tail::ARRAYFLATTENDEPTH,
            Divergence::MethodTypeDomain => established_tail::METHODTYPEDOMAIN,
            Divergence::ArrayJoinDomain => established_tail::ARRAYJOINDOMAIN,
            Divergence::FixedArraySpread => established_tail::FIXEDARRAYSPREAD,
            Divergence::ExplicitIntrinsicTypeArguments => {
                established_tail::EXPLICITINTRINSICTYPEARGUMENTS
            }
            Divergence::SourceConstructionDomain => established_tail::SOURCECONSTRUCTIONDOMAIN,
            Divergence::CallbackParameterShape => established_tail::CALLBACKPARAMETERSHAPE,
            Divergence::JsonCallArguments => established_tail::JSONCALLARGUMENTS,
            Divergence::JsonTypeDomain => established_tail::JSONTYPEDOMAIN,
            Divergence::StringSearchPattern => established_tail::STRINGSEARCHPATTERN,
            Divergence::MirrorParameterPattern => established_tail::MIRRORPARAMETERPATTERN,
            Divergence::LocaleNumberFormatting => established_tail::LOCALENUMBERFORMATTING,
            Divergence::UserIterationProtocol => established_tail::USERITERATIONPROTOCOL,
            Divergence::SetAlgebraDomain => established_tail::SETALGEBRADOMAIN,
            Divergence::VoidValue => established_tail::VOIDVALUE,
            Divergence::GeneratorDoneValue => established_tail::GENERATORDONEVALUE,
            Divergence::ReferenceSearchMiss => established_tail::REFERENCESEARCHMISS,
            Divergence::LoneSurrogateEscape => established_tail::LONESURROGATEESCAPE,
            Divergence::AnyType => established_tail::ANYTYPE,
            Divergence::DynamicObjectModel => established_tail::DYNAMICOBJECTMODEL,
            Divergence::NominalClassIdentity => established_tail::NOMINALCLASSIDENTITY,
            Divergence::ObjectLiteralConstruction => established_tail::OBJECTLITERALCONSTRUCTION,
            Divergence::ValueClassLayout => established_tail::VALUECLASSLAYOUT,
            Divergence::BareNumber => established_tail::BARENUMBER,
            Divergence::SizedOperandWidths => established_tail::SIZEDOPERANDWIDTHS,
            Divergence::StorageOnlyHalfFloat => established_tail::STORAGEONLYHALFFLOAT,
            Divergence::StringEnumMemberValue => super::type_flow::STRINGENUMMEMBERVALUE,
            Divergence::IntegerLiteralRange => established_tail::INTEGERLITERALRANGE,
            Divergence::WireEnumValues => established_tail::WIREENUMVALUES,
            Divergence::EscapingCapture => established_tail::ESCAPINGCAPTURE,
            Divergence::Exceptions => established_tail::EXCEPTIONS,
            Divergence::InstanceofNonError => established_tail::INSTANCEOFNONERROR,
            Divergence::GeneralUnionAndUndefined => established_tail::GENERALUNIONANDUNDEFINED,
            Divergence::NullishNonNullable => established_tail::NULLISHNONNULLABLE,
            Divergence::OptionalChainNonNullable => established_tail::OPTIONALCHAINNONNULLABLE,
            Divergence::NullishAssignment => established_tail::NULLISHASSIGNMENT,
            Divergence::NonPlaceNullishInitializer => established_tail::NONPLACENULLISHINITIALIZER,
            Divergence::OptionalChainUnbound => established_tail::OPTIONALCHAINUNBOUND,
            Divergence::OptionalChainIndex => established_tail::OPTIONALCHAININDEX,
            Divergence::LiteralUnionAlias => established_tail::LITERALUNIONALIAS,
            Divergence::OptionalDescriptorMember => established_tail::OPTIONALDESCRIPTORMEMBER,
            Divergence::BoundaryOnlyObject => established_tail::BOUNDARYONLYOBJECT,
            Divergence::PromiseObject => established_tail::PROMISEOBJECT,
            Divergence::AwaitOutsideAsync => established_tail::AWAITOUTSIDEASYNC,
            Divergence::AsyncArrowCapture => surface_forms::ASYNC_ARROW_CAPTURE,
            Divergence::AsyncReturnHandle => surface_forms::ASYNC_RETURN_HANDLE,
            Divergence::AsyncFunctionShape => established_tail::ASYNCFUNCTIONSHAPE,
            Divergence::DroppedAsyncHandle => established_tail::DROPPEDASYNCHANDLE,
            Divergence::ThisInFieldInitializer => established_tail::THISINFIELDINITIALIZER,
            Divergence::ClassIndexSignature => established_tail::CLASSINDEXSIGNATURE,
            Divergence::UsingDeclaration => established_tail::USINGDECLARATION,
            Divergence::NamedAccessor => established_tail::NAMEDACCESSOR,
            Divergence::IteratorTemporary => established_tail::ITERATORTEMPORARY,
            Divergence::HostApiSurface => established_tail::HOSTAPISURFACE,
            Divergence::NamedModuleSurface => established_tail::NAMEDMODULESURFACE,
            Divergence::DeclarationScope => established_tail::DECLARATIONSCOPE,
            Divergence::ModuleInitializerOrder => established_tail::MODULEINITIALIZERORDER,
            Divergence::GrowingInstanceChain => established_tail::GROWINGINSTANCECHAIN,
            Divergence::StaticMemberSurface => established_tail::STATICMEMBERSURFACE,
            Divergence::MathSubset => established_tail::MATHSUBSET,
            Divergence::DateSubset => established_tail::DATESUBSET,
            Divergence::LocaleSensitiveString => established_tail::LOCALESENSITIVESTRING,
            Divergence::ArrayMethodDefaults => established_tail::ARRAYMETHODDEFAULTS,
            Divergence::VariadicArguments => established_tail::VARIADICARGUMENTS,
            Divergence::MapKeyKind => established_tail::MAPKEYKIND,
            Divergence::MapScalarGet => established_tail::MAPSCALARGET,
            Divergence::SharedLocationNarrowing => established_tail::SHAREDLOCATIONNARROWING,
            Divergence::MapNonNullableGet => established_tail::MAPNONNULLABLEGET,
            Divergence::NoTupleType => established_tail::NOTUPLETYPE,
            Divergence::NumberCoercionAndArguments => established_tail::NUMBERCOERCIONANDARGUMENTS,
            Divergence::JsonSubset => established_tail::JSONSUBSET,
            Divergence::AggregateLayoutLimit => established_tail::AGGREGATELAYOUTLIMIT,
            Divergence::RegExpSubset => established_tail::REGEXPSUBSET,
            Divergence::ReplaceAllGlobalFlag => established_tail::REPLACEALLGLOBALFLAG,
            Divergence::WorkerEntryShape => established_tail::WORKERENTRYSHAPE,
            Divergence::WorkerContextAffinity => established_tail::WORKERCONTEXTAFFINITY,
            Divergence::SwitchOverAlias => established_tail::SWITCHOVERALIAS,
            Divergence::UnreachableInValuePosition => established_tail::UNREACHABLEINVALUEPOSITION,
            Divergence::DescriptorConstruction => established_tail::DESCRIPTORCONSTRUCTION,
            Divergence::ByteAccessTarget => established_tail::BYTEACCESSTARGET,
            Divergence::EntryParameterType => established_tail::ENTRYPARAMETERTYPE,
            Divergence::EmbeddedHeaderCopy => established_tail::EMBEDDEDHEADERCOPY,
            Divergence::GenericInferenceCandidates => established_tail::GENERICINFERENCECANDIDATES,
            Divergence::GenericInferenceMissing => established_tail::GENERICINFERENCEMISSING,
            Divergence::GenericMethodTypeArguments => established_tail::GENERICMETHODTYPEARGUMENTS,
            Divergence::BodilessDeclareGenericMethod => {
                established_tail::BODILESSDECLAREGENERICMETHOD
            }
            Divergence::GenericMethodOnGenericClass => {
                established_tail::GENERICMETHODONGENERICCLASS
            }
            Divergence::GeneratorSingleUse => established_tail::GENERATORSINGLEUSE,
            Divergence::BareMapSubject => established_tail::BAREMAPSUBJECT,
            Divergence::BareMapToArray => established_tail::BAREMAPTOARRAY,
            Divergence::ArrayFromMapper => established_tail::ARRAYFROMMAPPER,
            Divergence::ArrayIsArray => established_tail::ARRAYISARRAY,
            Divergence::ArrayOfArity => established_tail::ARRAYOFARITY,
            Divergence::ArrayHoleConstruction => established_tail::ARRAYHOLECONSTRUCTION,
            Divergence::PatternDefaultValue => established_tail::PATTERNDEFAULTVALUE,
            Divergence::ArrayRestPattern => established_tail::ARRAYRESTPATTERN,
            Divergence::ObjectRestPattern => established_tail::OBJECTRESTPATTERN,
            Divergence::NestedPattern => established_tail::NESTEDPATTERN,
            Divergence::PatternFieldName => established_tail::PATTERNFIELDNAME,
            Divergence::PatternSourceShape => established_tail::PATTERNSOURCESHAPE,
            Divergence::AssignmentPattern => established_tail::ASSIGNMENTPATTERN,
            Divergence::ModuleLevelPattern => established_tail::MODULELEVELPATTERN,
            Divergence::DefiniteAssignmentAssertion => {
                established_tail::DEFINITEASSIGNMENTASSERTION
            }
            Divergence::NestedFieldAssignmentEveryNormalExit => {
                established_tail::NESTEDFIELDASSIGNMENTEVERYNORMALEXIT
            }
            Divergence::AmbientClassConstruction => established_tail::AMBIENTCLASSCONSTRUCTION,
            Divergence::ThisBeforeFieldValues => established_tail::THISBEFOREFIELDVALUES,
        }
    }
}

#[path = "builtin_calls.rs"]
mod builtin_calls;

#[path = "surface_forms.rs"]
mod surface_forms;

#[path = "expression_forms.rs"]
mod expression_forms;

#[path = "established.rs"]
mod established;

#[path = "established_tail.rs"]
mod established_tail;
