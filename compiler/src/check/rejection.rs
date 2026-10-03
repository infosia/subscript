//! Named subset rejection sites and their accepted TypeScript classes.

use super::Checker;
use crate::diag::{Diagnostic, Pos, RuleCode};
use crate::divergence::Divergence;

#[path = "rejection_sites.rs"]
mod sites;
pub(crate) use sites::RejectionSite;

impl RejectionSite {
    pub(crate) fn class(self) -> (RuleCode, RejectionClass) {
        use RejectionClass::{Diverges, TscRejects};
        match self {
            Self::ErasedAssignableEquality => (
                RuleCode::S100,
                Diverges(Divergence::ErasedAssignableEquality),
            ),
            Self::DeclaredFieldWithoutValue => (
                RuleCode::S100,
                Diverges(Divergence::DeclaredFieldWithoutValue),
            ),
            Self::ErasedAssignableTypeMismatch => (
                RuleCode::S100,
                Diverges(Divergence::ErasedAssignableTypeMismatch),
            ),
            Self::NonNullableNullEquality => (
                RuleCode::S100,
                Diverges(Divergence::NonNullableNullEquality),
            ),
            Self::ThisInMethodArrow => (RuleCode::S100, Diverges(Divergence::ThisInMethodArrow)),
            Self::SwitchCaseClosureRead => {
                (RuleCode::S100, Diverges(Divergence::SwitchCaseClosureRead))
            }
            Self::EnumObjectMember => (RuleCode::S018, Diverges(Divergence::EnumObjectMember)),
            Self::ConstructorFieldReadAfterNestedAssignment => (
                RuleCode::S100,
                Diverges(Divergence::ConstructorFieldReadWithAssignmentFact),
            ),
            Self::MirrorArrayParameterNonIterable => (RuleCode::S100, TscRejects),
            Self::ExportAssignmentEsModule => (RuleCode::S100, TscRejects),
            Self::ExternalImportEqualsDeclaration => (RuleCode::S100, TscRejects),
            Self::DescriptorRequiredFieldUnassigned => (RuleCode::S100, TscRejects),
            Self::ComputedFieldUnboundName => (RuleCode::S100, TscRejects),
            Self::TopLevelOverloadImplementationMissing => (RuleCode::S017, TscRejects),
            Self::DuplicateNumericIndexSignature => (RuleCode::S100, TscRejects),
            Self::WireEnumMethodMember => (RuleCode::S100, TscRejects),
            Self::UnionUnknownTypeMember => (RuleCode::S011, TscRejects),
            Self::BuiltinDescriptorOptions => (RuleCode::S100, TscRejects),
            Self::BuiltinValueTypeAlignmentOutsideSet => (RuleCode::S100, TscRejects),
            Self::CollectionCallbackNotFunction => (RuleCode::S100, TscRejects),
            Self::InstancePrototypeMissing => (RuleCode::S003, TscRejects),
            Self::PoisonedRelativeDefaultImport => (RuleCode::S100, TscRejects),
            Self::AsyncReturnSourceNotPromise => (RuleCode::S100, TscRejects),
            Self::AsyncReturnNotPromise => (RuleCode::S100, TscRejects),
            Self::DuplicateTopLevelClass => (RuleCode::S017, TscRejects),
            Self::IteratorMethodValueArgumentCount => (RuleCode::S100, TscRejects),
            Self::UsingBindingNotDisposable => (RuleCode::S100, TscRejects),
            Self::ClassMemberDuplicateImplementation => (RuleCode::S017, TscRejects),
            Self::GenericMethodImplementationMissing => (RuleCode::S100, TscRejects),
            Self::FunctionImplementationMissing => (RuleCode::S100, TscRejects),
            Self::BuiltinArrayTypeArgumentCount => (RuleCode::S100, TscRejects),
            Self::LambdaReturnPathMissing => (RuleCode::S100, TscRejects),
            Self::FunctionReturnPathMissing => (RuleCode::S100, TscRejects),
            Self::DisjointLiteralAliasAssignment => (RuleCode::S100, TscRejects),
            Self::ExplicitVoidReturnValue => (RuleCode::S100, TscRejects),
            Self::WorkerMessageNonClass => (RuleCode::S100, TscRejects),
            Self::BindingPatternNonIterableSource => (RuleCode::S100, TscRejects),
            Self::NamedImportUnresolvedModule => (RuleCode::S100, TscRejects),
            Self::GenericInferenceIncompatibleKinds => (RuleCode::S100, TscRejects),
            Self::GenericInferenceRequiredArgumentMissing => (RuleCode::S100, TscRejects),
            Self::ErrorMessageNonString => (RuleCode::S100, TscRejects),
            Self::InstanceofLeftPrimitive => (RuleCode::S100, TscRejects),
            Self::InstanceofRightNotClass => (RuleCode::S100, TscRejects),
            Self::CatchBindingPatternWithoutAny => (RuleCode::S010, TscRejects),
            Self::CatchBindingInvalidAnnotation => (RuleCode::S010, TscRejects),
            Self::ImmediateNameWriteBeforeDeclaration => (RuleCode::S100, TscRejects),
            Self::ImmediateShadowedNameRead => (RuleCode::S100, TscRejects),
            Self::ConstructorFieldReadUnassigned => (RuleCode::S100, TscRejects),
            Self::FieldAssignmentNestedUnassignedExit => (RuleCode::S100, TscRejects),
            Self::FieldAssignmentMissingUnassignedExit => (RuleCode::S100, TscRejects),
            Self::FieldAssignmentAfterReturnUnassignedExit => (RuleCode::S100, TscRejects),
            Self::TypeParameterDefault => {
                (RuleCode::S100, Diverges(Divergence::TypeParameterDefault))
            }
            Self::MapCopyNullableNonMap => (RuleCode::S011, TscRejects),

            Self::ClassUndeclaredPropertyWrite => (RuleCode::S004, TscRejects),
            Self::ClassUndeclaredMemberRead => (RuleCode::S018, TscRejects),
            Self::ClassUndeclaredMethodCall => (RuleCode::S018, TscRejects),
            Self::AwaitClassUndeclaredMethod => (RuleCode::S018, TscRejects),
            Self::ClassStaticMemberMissing => (RuleCode::S018, TscRejects),
            Self::GenericClassStaticMemberMissing => (RuleCode::S018, TscRejects),

            Self::FieldInitializerDeclaredRead => (RuleCode::S100, TscRejects),
            Self::DescriptorDefaultOptionalNumericOperand => (RuleCode::S100, TscRejects),
            Self::BareYieldDeclaredNonVoid => (RuleCode::S100, TscRejects),
            Self::NamespaceIncompatibleContext => (RuleCode::S100, TscRejects),

            Self::FixedArrayUnknownMember => (RuleCode::S100, TscRejects),
            Self::MapUnknownMember => (RuleCode::S100, TscRejects),
            Self::SetUnknownMember => (RuleCode::S100, TscRejects),
            Self::GeneratorResultUnknownMember => (RuleCode::S100, TscRejects),
            Self::NumericUnknownMember => (RuleCode::S018, TscRejects),
            Self::BoundaryUnknownMember => (RuleCode::S100, TscRejects),
            Self::WorkerStaticUnknownMethod => (RuleCode::S018, TscRejects),
            Self::WorkerUnknownMethod => (RuleCode::S018, TscRejects),
            Self::InboxUnknownMethod => (RuleCode::S018, TscRejects),
            Self::OutboxUnknownMethod => (RuleCode::S018, TscRejects),
            Self::FixedArrayUnknownMethod => (RuleCode::S018, TscRejects),
            Self::NumericUnknownMethod => (RuleCode::S018, TscRejects),
            Self::StringUnknownMember => (RuleCode::S100, TscRejects),
            Self::MapUnknownMethod => (RuleCode::S100, TscRejects),
            Self::SetUnknownMethod => (RuleCode::S100, TscRejects),
            Self::ArrayUnknownMember => (RuleCode::S100, TscRejects),
            Self::RegexCompileUnknownMethod => (RuleCode::S100, TscRejects),
            Self::SynchronousMethodValue => (
                RuleCode::S100,
                Diverges(Divergence::SynchronousMethodValueForm),
            ),
            Self::GenericSynchronousMethodValue => (
                RuleCode::S100,
                Diverges(Divergence::GenericSynchronousMethodValueForm),
            ),
            Self::GeneratorFunctionValue => (
                RuleCode::S100,
                Diverges(Divergence::GeneratorFunctionValueForm),
            ),
            Self::YieldOutsideGenerator => (RuleCode::S100, TscRejects),
            Self::ByteArgumentTypeMismatch => (RuleCode::S100, TscRejects),
            Self::WorkerSpawnArgumentCount => (RuleCode::S100, TscRejects),
            Self::WorkerEntryParameterCount => (RuleCode::S100, TscRejects),
            Self::WorkerEntryEndpointMismatch => (RuleCode::S100, TscRejects),
            Self::ArrayCallbackArgumentCount => (RuleCode::S100, TscRejects),
            Self::MapCallbackArgumentCount => (RuleCode::S100, TscRejects),
            Self::SetCallbackArgumentCount => (RuleCode::S100, TscRejects),
            Self::DescriptorRequiredMemberMissing => (RuleCode::S100, TscRejects),

            Self::GenericConstraintMismatch => (RuleCode::S100, TscRejects),
            Self::NamespaceMemberMissing => (RuleCode::S016, TscRejects),
            Self::NullableCall => (RuleCode::S100, TscRejects),
            Self::NullableCallNonNullFlow => (
                RuleCode::S100,
                Diverges(Divergence::NullableCallNonNullFlow),
            ),
            Self::NullableCallShared => (
                RuleCode::S100,
                Diverges(Divergence::SharedLocationNarrowing),
            ),
            Self::NullableMember => (RuleCode::S011, TscRejects),
            Self::NullableMemberNonNullFlow => (
                RuleCode::S011,
                Diverges(Divergence::NullableMemberNonNullFlow),
            ),
            Self::NullableMemberShared => (
                RuleCode::S011,
                Diverges(Divergence::SharedLocationNarrowing),
            ),
            Self::UnknownNamespaceConstructor => {
                (RuleCode::S100, Diverges(Divergence::LibConstructorName))
            }
            Self::UnknownClassConstructor => (RuleCode::S016, TscRejects),
            Self::ThisStaticField => (RuleCode::S100, Diverges(Divergence::StaticMemberSurface)),
            Self::RunnerMainMissing => (RuleCode::S100, Diverges(Divergence::RunnerMainMissing)),
            Self::ContextAffineContainerArgument => (
                RuleCode::S100,
                Diverges(Divergence::ContextAffineContainerArgument),
            ),
            Self::GenericInferenceNoCandidate => (
                RuleCode::S100,
                Diverges(Divergence::GenericInferenceMissing),
            ),
            Self::GenericInferenceConflictingCandidates => (
                RuleCode::S100,
                Diverges(Divergence::GenericInferenceCandidates),
            ),
            Self::DuplicateLocalDeclaration => (RuleCode::S017, TscRejects),
            Self::IteratorBindingUnsupportedMembers => {
                (RuleCode::S100, Diverges(Divergence::PatternSourceShape))
            }
            Self::IteratorBindingUnknownMembers => (RuleCode::S100, TscRejects),
            Self::BindingPatternSourceKind => {
                (RuleCode::S100, Diverges(Divergence::PatternSourceShape))
            }
            Self::SourceFilesEmpty => (RuleCode::S100, TscRejects),
            Self::ParserLoneSurrogateEscape => {
                (RuleCode::S100, Diverges(Divergence::LoneSurrogateEscape))
            }
            Self::ParserSyntaxError => (RuleCode::S100, TscRejects),
            Self::ClassAlignmentBelowNatural => {
                (RuleCode::S100, Diverges(Divergence::ValueClassLayout))
            }
            Self::ClassFinalAlignmentLimit => (
                RuleCode::S100,
                Diverges(Divergence::ClassFinalAlignmentLimit),
            ),
            Self::ClassFieldLayoutLimit => {
                (RuleCode::S100, Diverges(Divergence::AggregateLayoutLimit))
            }
            Self::AggregateArgumentFrameLimit => (
                RuleCode::S100,
                Diverges(Divergence::AggregateArgumentFrameLimit),
            ),
            Self::SuspendFrameMemberLayoutLimit => {
                (RuleCode::S100, Diverges(Divergence::AggregateLayoutLimit))
            }
            Self::AsyncChildFrameLayoutLimit => {
                (RuleCode::S100, Diverges(Divergence::AggregateLayoutLimit))
            }
            Self::GeneratorFrameFinalAlignmentLimit => {
                (RuleCode::S100, Diverges(Divergence::AggregateLayoutLimit))
            }
            Self::ClosureEnvironmentLayoutLimit => {
                (RuleCode::S100, Diverges(Divergence::AggregateLayoutLimit))
            }
            Self::StoredAggregateLayoutLimit => {
                (RuleCode::S100, Diverges(Divergence::AggregateLayoutLimit))
            }
            Self::TypeOnlyImportValueUse => (RuleCode::S100, TscRejects),
            Self::NamespaceAsValue => (RuleCode::S100, TscRejects),
            Self::SwitchCaseRead => (RuleCode::S100, TscRejects),
            Self::SwitchCaseWriteOutsideDeclaration => {
                (RuleCode::S100, Diverges(Divergence::DeclarationScope))
            }
            Self::BlockNameReadBeforeDeclaration => (
                RuleCode::S100,
                Diverges(Divergence::BlockNameReadBeforeDeclaration),
            ),
            Self::BlockPendingReadWithoutProgramShadow => {
                (RuleCode::S100, Diverges(Divergence::DeclarationScope))
            }
            Self::BlockNameWriteBeforeDeclaration => (
                RuleCode::S100,
                Diverges(Divergence::BlockNameWriteBeforeDeclaration),
            ),
            Self::ContextAffineCapture => {
                (RuleCode::S100, Diverges(Divergence::ContextAffineCapture))
            }
            Self::MutableLocalCapture => {
                (RuleCode::S009, Diverges(Divergence::MutableLocalCapture))
            }
            Self::ModuleVarDeclaration => (
                RuleCode::S100,
                Diverges(Divergence::ModuleVarDeclarationForm),
            ),
            Self::ModuleInitializerMissing => (
                RuleCode::S100,
                Diverges(Divergence::ModuleInitializerMissingForm),
            ),
            Self::FunctionBodyMissing => {
                (RuleCode::S100, Diverges(Divergence::FunctionBodyMissing))
            }
            Self::AsyncHandleUnawaited => {
                (RuleCode::S013, Diverges(Divergence::DroppedAsyncHandle))
            }
            Self::FunctionReturnCoverage => {
                (RuleCode::S100, Diverges(Divergence::ReturnFlowCoverage))
            }
            Self::StaticFieldInitializerMissing => (
                RuleCode::S100,
                Diverges(Divergence::StaticFieldInitializerMissing),
            ),
            Self::FieldDefiniteAssertionUnassigned => (
                RuleCode::S100,
                Diverges(Divergence::DefiniteAssignmentAssertion),
            ),
            Self::FieldAssignmentAfterUnreachableReturn => (
                RuleCode::S100,
                Diverges(Divergence::FieldAssignmentAfterUnreachableReturn),
            ),
            Self::FieldAssignmentNestedEveryNormalExit => (
                RuleCode::S100,
                Diverges(Divergence::NestedFieldAssignmentEveryNormalExit),
            ),
            Self::FieldAssignmentMissingNoNormalExit => (
                RuleCode::S100,
                Diverges(Divergence::FieldAssignmentMissingNoNormalExit),
            ),
            Self::ConstructorDefiniteFieldReadBeforeAssignment => (
                RuleCode::S100,
                Diverges(Divergence::ConstructorFieldReadWithAssignmentFact),
            ),
            Self::ConstructorThisBeforeFieldValues => {
                (RuleCode::S100, Diverges(Divergence::ThisBeforeFieldValues))
            }
            Self::LambdaUsingDeclaration => {
                (RuleCode::S100, Diverges(Divergence::UsingDeclaration))
            }
            Self::LocalClassDeclaration => {
                (RuleCode::S100, Diverges(Divergence::LocalClassDeclaration))
            }
            Self::LocalFunctionDeclaration => (
                RuleCode::S100,
                Diverges(Divergence::LocalFunctionDeclaration),
            ),
            Self::LocalEnumDeclaration => {
                (RuleCode::S100, Diverges(Divergence::LocalEnumDeclaration))
            }
            Self::LocalAliasDeclaration => {
                (RuleCode::S100, Diverges(Divergence::LocalAliasDeclaration))
            }
            Self::LocalInterfaceDeclaration => (
                RuleCode::S100,
                Diverges(Divergence::LocalInterfaceDeclaration),
            ),
            Self::LocalNamespaceDeclaration => (RuleCode::S100, TscRejects),
            Self::LocalRejectedDeclaration => (RuleCode::S100, TscRejects),
            Self::LabeledBreak => (RuleCode::S100, TscRejects),
            Self::BreakOutsideLoopOrSwitch => (RuleCode::S100, TscRejects),
            Self::LabeledContinue => (RuleCode::S100, TscRejects),
            Self::ContinueOutsideLoop => (RuleCode::S100, TscRejects),
            Self::DoWhileStatement => (RuleCode::S100, Diverges(Divergence::DoWhileStatement)),
            Self::ForInStatement => (RuleCode::S100, Diverges(Divergence::ForInStatement)),
            Self::LabeledStatement => (RuleCode::S100, Diverges(Divergence::LabeledStatement)),
            Self::DebuggerStatement => (RuleCode::S100, Diverges(Divergence::DebuggerStatement)),
            Self::WithStatement => (RuleCode::S100, TscRejects),
            Self::UnsupportedStatementKind => (RuleCode::S100, TscRejects),
            Self::LocalVarDeclaration => (
                RuleCode::S100,
                Diverges(Divergence::LocalVarDeclarationForm),
            ),
            Self::AwaitUsingDeclaration => (RuleCode::S100, Diverges(Divergence::UsingDeclaration)),
            Self::LocalInitializerMissing => (
                RuleCode::S100,
                Diverges(Divergence::LocalInitializerMissingForm),
            ),
            Self::NullInitializerInference => (
                RuleCode::S100,
                Diverges(Divergence::NullInitializerInference),
            ),
            Self::UsingBindingResourceType => (
                RuleCode::S100,
                Diverges(Divergence::UsingBindingResourceType),
            ),
            Self::GeneratorReturnValue => {
                (RuleCode::S100, Diverges(Divergence::GeneratorReturnValue))
            }
            Self::VoidFunctionReturnValue => (RuleCode::S100, Diverges(Divergence::VoidValue)),
            Self::ReturnValueMissing => (RuleCode::S100, TscRejects),
            Self::StatementNonBooleanCondition => (
                RuleCode::S100,
                Diverges(Divergence::StatementNonBooleanConditionForm),
            ),
            Self::AsyncForOf => (RuleCode::S013, Diverges(Divergence::AsyncForOfForm)),
            Self::ForOfVarBinding => (RuleCode::S100, Diverges(Divergence::ForOfVarBindingForm)),
            Self::ForOfAwaitUsing => (RuleCode::S100, Diverges(Divergence::ForOfAwaitUsing)),
            Self::ForOfBindingKind => (RuleCode::S100, Diverges(Divergence::ForOfBindingKindForm)),
            Self::ForOfBindingCount => (RuleCode::S100, TscRejects),
            Self::ForOfBindingInitializer => (RuleCode::S100, TscRejects),
            Self::IteratorMethodArgumentCount => {
                (RuleCode::S100, Diverges(Divergence::ForOfSpreadCall))
            }
            Self::LiteralAliasDuplicateCase => {
                (RuleCode::S100, Diverges(Divergence::SwitchOverAlias))
            }
            Self::LiteralAliasUnknownCase => (RuleCode::S100, TscRejects),
            Self::AliasCaseNonLiteral => {
                (RuleCode::S100, Diverges(Divergence::AliasCaseNonLiteral))
            }
            Self::SwitchDiscriminantKind => (
                RuleCode::S100,
                Diverges(Divergence::SwitchDiscriminantKindForm),
            ),
            Self::LiteralAliasSwitchCoverage => {
                (RuleCode::S100, Diverges(Divergence::SwitchOverAlias))
            }
            Self::DuplicateReadAccessor => (RuleCode::S017, TscRejects),
            Self::DuplicateWriteAccessor => (RuleCode::S017, TscRejects),
            Self::ClassMemberNameClash => {
                (RuleCode::S017, Diverges(Divergence::ClassMemberNameClash))
            }
            Self::ComputedMethodName => (RuleCode::S100, Diverges(Divergence::ComputedMethodName)),
            Self::DescriptorDisposeMethod => {
                (RuleCode::S100, Diverges(Divergence::UsingDeclaration))
            }
            Self::DescriptorMethod => (RuleCode::S100, Diverges(Divergence::DescriptorMethod)),
            Self::MirrorStaticMethod => (RuleCode::S100, Diverges(Divergence::MirrorStaticMethod)),
            Self::DisposeStatic => (RuleCode::S100, Diverges(Divergence::DisposeStatic)),
            Self::AsyncStaticMethod => (RuleCode::S100, Diverges(Divergence::AsyncFunctionShape)),
            Self::ValueClassDisposeMethod => {
                (RuleCode::S100, Diverges(Divergence::UsingDeclaration))
            }
            Self::MirrorAccessor => (RuleCode::S100, Diverges(Divergence::MirrorAccessor)),
            Self::ReadAccessorParameters => (RuleCode::S100, TscRejects),
            Self::ReadAccessorReturnMissing => (
                RuleCode::S100,
                Diverges(Divergence::ReadAccessorReturnMissing),
            ),
            Self::ValueClassWriteAccessor => (RuleCode::S100, Diverges(Divergence::NamedAccessor)),
            Self::WriteAccessorReturnAnnotation => (RuleCode::S100, TscRejects),
            Self::WriteAccessorParameterCount => (RuleCode::S100, TscRejects),
            Self::WriteAccessorDefaultParameter => (RuleCode::S100, TscRejects),
            Self::WriteAccessorPattern => {
                (RuleCode::S100, Diverges(Divergence::WriteAccessorPattern))
            }
            Self::WriteAccessorTypeMissing => (
                RuleCode::S100,
                Diverges(Divergence::WriteAccessorTypeMissing),
            ),
            Self::AsyncGeneratorMethod => {
                (RuleCode::S100, Diverges(Divergence::AsyncFunctionShape))
            }
            Self::GeneratorMethodDeclaration => {
                (RuleCode::S100, Diverges(Divergence::GeneratorMethodForm))
            }
            Self::ValueClassAsyncMethod => {
                (RuleCode::S100, Diverges(Divergence::AsyncFunctionShape))
            }
            Self::MethodBodyMissing => (
                RuleCode::S100,
                Diverges(Divergence::BodilessDeclareGenericMethod),
            ),
            Self::GenericMethodBodyMissing => (
                RuleCode::S100,
                Diverges(Divergence::GenericMethodBodyMissing),
            ),
            Self::DisposeAsync => (RuleCode::S100, Diverges(Divergence::DisposeAsync)),
            Self::DisposeSignature => (RuleCode::S100, Diverges(Divergence::DisposeSignature)),
            Self::ValueClassInheritance => (RuleCode::S006, Diverges(Divergence::ValueClassLayout)),
            Self::DescriptorInheritance => {
                (RuleCode::S100, Diverges(Divergence::DescriptorInheritance))
            }
            Self::ReferenceClassInheritance => (
                RuleCode::S100,
                Diverges(Divergence::ReferenceClassInheritance),
            ),
            Self::ComputedFieldDeclaration => {
                (RuleCode::S100, Diverges(Divergence::IdentifierFieldName))
            }
            Self::DescriptorStaticField => {
                (RuleCode::S100, Diverges(Divergence::DescriptorStaticField))
            }
            Self::MirrorStaticField => (RuleCode::S100, Diverges(Divergence::MirrorStaticField)),
            Self::StaticFieldOptional => {
                (RuleCode::S012, Diverges(Divergence::StaticFieldOptional))
            }
            Self::StaticFieldAnnotationMissing => (
                RuleCode::S100,
                Diverges(Divergence::StaticFieldAnnotationMissingForm),
            ),
            Self::ContextAffineStaticField => (
                RuleCode::S100,
                Diverges(Divergence::ContextAffineStaticField),
            ),
            Self::DescriptorOptionalDefaultMissing => (
                RuleCode::S012,
                Diverges(Divergence::OptionalDescriptorMember),
            ),
            Self::DescriptorRequiredInitializer => (RuleCode::S100, TscRejects),
            Self::DescriptorInitializerWithoutOptional => (
                RuleCode::S100,
                Diverges(Divergence::DescriptorInitializerWithoutOptional),
            ),
            Self::DescriptorRequiredWithoutDefinite => (
                RuleCode::S100,
                Diverges(Divergence::DescriptorRequiredWithoutDefinite),
            ),
            Self::InstanceFieldOptional => {
                (RuleCode::S012, Diverges(Divergence::InstanceFieldOptional))
            }
            Self::InstanceFieldAnnotationMissing => (
                RuleCode::S100,
                Diverges(Divergence::InstanceFieldAnnotationMissingForm),
            ),
            Self::WireAliasNestedField => {
                (RuleCode::S100, Diverges(Divergence::WireAliasNestedField))
            }
            Self::DescriptorOptionalInitializerMissing => (
                RuleCode::S012,
                Diverges(Divergence::OptionalDescriptorMember),
            ),
            Self::ContextAffineInstanceField => (
                RuleCode::S100,
                Diverges(Divergence::ContextAffineInstanceField),
            ),
            Self::ValueFieldOutsideWhitelist => (
                RuleCode::S100,
                Diverges(Divergence::ValueFieldOutsideWhitelist),
            ),
            Self::DescriptorConstructor => {
                (RuleCode::S100, Diverges(Divergence::DescriptorConstructor))
            }
            Self::WireAliasNestedConstructorParameter => (
                RuleCode::S100,
                Diverges(Divergence::WireAliasNestedConstructorParameter),
            ),
            Self::ConstructorParameterProperty => (
                RuleCode::S100,
                Diverges(Divergence::ConstructorParameterPropertyForm),
            ),
            Self::ClassIndexSignatureCount => (
                RuleCode::S100,
                Diverges(Divergence::ClassIndexSignatureCount),
            ),
            Self::ClassIndexSignatureNonReference => (
                RuleCode::S100,
                Diverges(Divergence::ClassIndexSignatureNonReference),
            ),
            Self::ClassIndexSignatureStatic => (
                RuleCode::S100,
                Diverges(Divergence::ClassIndexSignatureStatic),
            ),
            Self::IndexSignatureParameterAnnotationMissing => (RuleCode::S100, TscRejects),
            Self::IndexSignatureParameterKind => (RuleCode::S100, TscRejects),
            Self::ClassIndexSignatureIndexType => (
                RuleCode::S100,
                Diverges(Divergence::ClassIndexSignatureIndexType),
            ),
            Self::IndexSignatureElementAnnotationMissing => (RuleCode::S100, TscRejects),
            Self::PrivateFieldDeclaration => (
                RuleCode::S100,
                Diverges(Divergence::PrivateFieldDeclaration),
            ),
            Self::PrivateMethodDeclaration => (
                RuleCode::S100,
                Diverges(Divergence::PrivateMethodDeclaration),
            ),
            Self::StaticBlockDeclaration => {
                (RuleCode::S100, Diverges(Divergence::StaticBlockDeclaration))
            }
            Self::AutoAccessorDeclaration => (
                RuleCode::S100,
                Diverges(Divergence::AutoAccessorDeclaration),
            ),
            Self::UnsupportedClassMemberKind => (RuleCode::S100, TscRejects),
            Self::WriteAccessorWithoutRead => (
                RuleCode::S100,
                Diverges(Divergence::WriteAccessorWithoutRead),
            ),
            Self::AccessorTypeMismatch => {
                (RuleCode::S100, Diverges(Divergence::AccessorTypeMismatch))
            }
            Self::IndexSignatureGetterMismatch => {
                (RuleCode::S100, Diverges(Divergence::ClassIndexSignature))
            }
            Self::ClassIndexSetSignature => {
                (RuleCode::S100, Diverges(Divergence::ClassIndexSetSignature))
            }
            Self::DuplicateDirectExport => (RuleCode::S017, TscRejects),
            Self::MirrorExportList => (RuleCode::S100, Diverges(Divergence::MirrorExportList)),
            Self::TypeOnlyExportDeclaration => {
                (RuleCode::S100, Diverges(Divergence::NamedModuleSurface))
            }
            Self::ExportSourceModuleMissing => (RuleCode::S100, TscRejects),
            Self::ExportSpecifierKind => (RuleCode::S100, Diverges(Divergence::NamedModuleSurface)),
            Self::TypeOnlyOrDefaultExportSpecifier => {
                (RuleCode::S100, Diverges(Divergence::NamedModuleSurface))
            }
            Self::TypeOnlyImportReexport => {
                (RuleCode::S100, Diverges(Divergence::NamedModuleSurface))
            }
            Self::DuplicateAliasedExport => (RuleCode::S017, TscRejects),
            Self::ExportAliasCycle => (RuleCode::S016, TscRejects),
            Self::ReexportSpecifierKind => {
                (RuleCode::S100, Diverges(Divergence::NamedModuleSurface))
            }
            Self::ExportLocalMissing => (RuleCode::S016, TscRejects),
            Self::ReexportMemberMissing => (RuleCode::S016, TscRejects),
            Self::ErrorConstructorTypeArguments => (RuleCode::S100, TscRejects),
            Self::ErrorMessageType => (RuleCode::S100, Diverges(Divergence::ErrorMessageType)),
            Self::ErrorConstructorArguments => (
                RuleCode::S100,
                Diverges(Divergence::ErrorConstructorArguments),
            ),
            Self::ErrorCallWithoutNew => {
                (RuleCode::S100, Diverges(Divergence::ErrorCallWithoutNew))
            }
            Self::CatchBindingUnnarrowedUse => (RuleCode::S010, Diverges(Divergence::Exceptions)),
            Self::ThrowOperandNotErrorFamily => (RuleCode::S010, Diverges(Divergence::Exceptions)),
            Self::FinallyClause => (RuleCode::S010, Diverges(Divergence::Exceptions)),
            Self::CatchBindingPattern => (RuleCode::S010, Diverges(Divergence::Exceptions)),
            Self::CatchBindingAnnotation => (RuleCode::S010, Diverges(Divergence::Exceptions)),
            Self::InstanceofRightNotErrorFamily => {
                (RuleCode::S100, Diverges(Divergence::InstanceofNonError))
            }
            Self::InstanceofLeftNotErrorFamily => {
                (RuleCode::S100, Diverges(Divergence::InstanceofNonError))
            }
            Self::InvalidProgramEntry => {
                (RuleCode::S100, Diverges(Divergence::InvalidProgramEntry))
            }
            Self::HostEntrySignatureMismatch => {
                (RuleCode::S100, Diverges(Divergence::HostApiSurface))
            }
            Self::GenericConstraintIdentity => (
                RuleCode::S100,
                Diverges(Divergence::GenericConstraintIdentity),
            ),
            Self::GenericConstraintCycle => (RuleCode::S100, TscRejects),
            Self::GenericFunctionTypeArgumentCount => (RuleCode::S100, TscRejects),
            Self::GenericMethodTypeArgumentCount => (RuleCode::S100, TscRejects),
            Self::GenericClassTypeArgumentCount => (RuleCode::S100, TscRejects),
            Self::DeferredInstanceArgumentsMissing => (RuleCode::S100, TscRejects),
            Self::DeferredInstanceTemplateMissing => (RuleCode::S100, TscRejects),
            Self::MirrorHeaderMissing => {
                (RuleCode::S100, Diverges(Divergence::MirrorHeaderMissing))
            }
            Self::MirrorParameterTargetMissing => (
                RuleCode::S100,
                Diverges(Divergence::MirrorParameterTargetMissing),
            ),
            Self::MirrorCallbackTargetMissing => (
                RuleCode::S100,
                Diverges(Divergence::MirrorCallbackTargetMissing),
            ),
            Self::MirrorLifetimeTargetMissing => (
                RuleCode::S100,
                Diverges(Divergence::MirrorLifetimeTargetMissing),
            ),
            Self::MirrorArrayProvenanceMissing => (
                RuleCode::S100,
                Diverges(Divergence::MirrorArrayProvenanceMissing),
            ),
            Self::MirrorStringProvenanceMissing => (
                RuleCode::S100,
                Diverges(Divergence::MirrorStringProvenanceMissing),
            ),
            Self::MirrorParameterProvenanceMismatch => (
                RuleCode::S100,
                Diverges(Divergence::MirrorParameterProvenanceMismatch),
            ),
            Self::MirrorAnonymousCallback => (
                RuleCode::S100,
                Diverges(Divergence::MirrorAnonymousCallback),
            ),
            Self::MirrorCallbackProvenanceMissing => (
                RuleCode::S100,
                Diverges(Divergence::MirrorCallbackProvenanceMissing),
            ),
            Self::InitializerNonPlaceOptionalReceiver => (
                RuleCode::S100,
                Diverges(Divergence::NonPlaceNullishInitializer),
            ),
            Self::GenericInstanceExpansionLimit => {
                (RuleCode::S100, Diverges(Divergence::GrowingInstanceChain))
            }
            Self::TopLevelNameClash => (RuleCode::S017, Diverges(Divergence::TopLevelNameClash)),
            Self::DefaultExportDeclaration => {
                (RuleCode::S100, Diverges(Divergence::NamedModuleSurface))
            }
            Self::UnsupportedModuleDeclaration => (
                RuleCode::S100,
                Diverges(Divergence::UnsupportedModuleDeclaration),
            ),
            Self::ModuleUsing => (RuleCode::S100, Diverges(Divergence::ModuleUsing)),
            Self::SourceInterfaceDeclaration => (
                RuleCode::S100,
                Diverges(Divergence::SourceInterfaceDeclaration),
            ),
            Self::SourceNamespaceDeclaration => (
                RuleCode::S100,
                Diverges(Divergence::SourceNamespaceDeclaration),
            ),
            Self::DescriptorOptions => (RuleCode::S100, Diverges(Divergence::DescriptorOptions)),
            Self::UnsupportedClassDecorator => (
                RuleCode::S100,
                Diverges(Divergence::UnsupportedClassDecorator),
            ),
            Self::DescriptorValueType => {
                (RuleCode::S100, Diverges(Divergence::DescriptorValueType))
            }
            Self::GenericClassStaticMember => {
                (RuleCode::S100, Diverges(Divergence::StaticMemberSurface))
            }
            Self::GenericClassGenericMethod => (
                RuleCode::S100,
                Diverges(Divergence::GenericMethodOnGenericClass),
            ),
            Self::SourceFunctionBodyMissing => (RuleCode::S100, TscRejects),
            Self::DuplicateTypeParameter => (RuleCode::S017, TscRejects),
            Self::ModuleBindingPattern => {
                (RuleCode::S100, Diverges(Divergence::ModuleLevelPattern))
            }
            Self::EnumStringMemberName => (
                RuleCode::S100,
                Diverges(Divergence::EnumStringMemberNameForm),
            ),
            Self::EnumImplicitValueOverflow => (
                RuleCode::S008,
                Diverges(Divergence::EnumImplicitValueOverflow),
            ),
            Self::EnumStringValue => (RuleCode::S100, Diverges(Divergence::StringEnumMemberValue)),
            Self::EnumNonIntegerMember => {
                (RuleCode::S100, Diverges(Divergence::IntegerLiteralRange))
            }
            Self::GenericSourceAlias => {
                (RuleCode::S100, Diverges(Divergence::GenericSourceAliasForm))
            }
            Self::SourceAliasNotLiteralUnion => (
                RuleCode::S100,
                Diverges(Divergence::SourceAliasNotLiteralUnionForm),
            ),
            Self::LiteralAliasDiscriminantLimit => (RuleCode::S100, TscRejects),
            Self::DuplicateLiteralAliasMember => (
                RuleCode::S100,
                Diverges(Divergence::DuplicateLiteralAliasMemberForm),
            ),
            Self::WireEnumEmpty => (RuleCode::S100, Diverges(Divergence::WireEnumEmpty)),
            Self::WireAliasDiscriminantLimit => (RuleCode::S100, TscRejects),
            Self::WireEnumMemberForm => (RuleCode::S100, Diverges(Divergence::WireEnumMemberForm)),
            Self::WireEnumMemberKey => (RuleCode::S100, Diverges(Divergence::WireEnumMemberKey)),
            Self::DuplicateWireAliasMember => (RuleCode::S100, TscRejects),
            Self::WireEnumUntypedMember => (RuleCode::S100, TscRejects),
            Self::WireEnumMemberNonIntegerSyntax => {
                (RuleCode::S100, Diverges(Divergence::WireEnumValues))
            }
            Self::WireEnumMemberNonIntegerValue => {
                (RuleCode::S100, Diverges(Divergence::WireEnumValues))
            }
            Self::WireEnumValueRange => (RuleCode::S100, Diverges(Divergence::WireEnumValues)),
            Self::DuplicateWireEnumValue => (RuleCode::S100, Diverges(Divergence::WireEnumValues)),
            Self::MirrorModuleDeclaration => (
                RuleCode::S100,
                Diverges(Divergence::MirrorModuleDeclarationForm),
            ),
            Self::MirrorVariableForm => (RuleCode::S100, Diverges(Divergence::MirrorVariableForm)),
            Self::InitializerDirectRead => (RuleCode::S100, TscRejects),
            Self::InitializerRouteRead => {
                (RuleCode::S100, Diverges(Divergence::ModuleInitializerOrder))
            }
            Self::ContextAffineArrayElement => (
                RuleCode::S100,
                Diverges(Divergence::ContextAffineArrayElement),
            ),
            Self::UriCodecTypeArguments => (RuleCode::S100, TscRejects),
            Self::WireAliasNestedForeignParameter => (
                RuleCode::S100,
                Diverges(Divergence::WireAliasNestedForeignParameter),
            ),
            Self::WireAliasNestedForeignReturn => (
                RuleCode::S100,
                Diverges(Divergence::WireAliasNestedForeignReturn),
            ),
            Self::ForeignDirectCallback => {
                (RuleCode::S100, Diverges(Divergence::ForeignDirectCallback))
            }
            Self::ForeignReturnProvenance => (
                RuleCode::S100,
                Diverges(Divergence::ForeignReturnProvenance),
            ),
            Self::ForeignFunctionHeaderMissing => (RuleCode::S100, TscRejects),
            Self::TypeOnlyNamespaceImport => {
                (RuleCode::S100, Diverges(Divergence::NamedModuleSurface))
            }
            Self::NamespaceImportTargetMissing => (
                RuleCode::S100,
                Diverges(Divergence::NamespaceImportTargetMissingForm),
            ),
            Self::PoisonedDefaultImport => {
                (RuleCode::S100, Diverges(Divergence::PoisonedDefaultImport))
            }
            Self::NamedImportModuleMissing => (
                RuleCode::S100,
                Diverges(Divergence::NamedImportModuleMissingForm),
            ),
            Self::DefaultImport => (RuleCode::S100, Diverges(Divergence::DefaultImport)),
            Self::NamedImportMemberMissing => (RuleCode::S016, TscRejects),
            Self::ModuleVariableAnnotationMissing => (
                RuleCode::S100,
                Diverges(Divergence::ModuleVariableAnnotationMissingForm),
            ),
            Self::WorkerEndpointModuleGlobal => {
                (RuleCode::S100, Diverges(Divergence::WorkerContextAffinity))
            }
            Self::AsyncGeneratorFunction => {
                (RuleCode::S100, Diverges(Divergence::AsyncGeneratorFunction))
            }
            Self::AsyncReturnAnnotationMissing => (
                RuleCode::S100,
                Diverges(Divergence::AsyncReturnAnnotationMissing),
            ),
            Self::FunctionReturnAnnotationMissing => (
                RuleCode::S100,
                Diverges(Divergence::FunctionReturnAnnotationMissingForm),
            ),
            Self::OptionalParameter => (RuleCode::S012, Diverges(Divergence::OptionalParameter)),
            Self::NamedParameterAnnotationMissing => (
                RuleCode::S100,
                Diverges(Divergence::NamedParameterAnnotationMissingForm),
            ),
            Self::PatternParameterAnnotationMissing => (
                RuleCode::S100,
                Diverges(Divergence::PatternParameterAnnotationMissingForm),
            ),
            Self::AsyncReturnNonReference => (
                RuleCode::S100,
                Diverges(Divergence::AsyncReturnNonReference),
            ),
            Self::AsyncReturnQualifiedName => (
                RuleCode::S100,
                Diverges(Divergence::AsyncReturnQualifiedName),
            ),
            Self::AsyncReturnAlias => (RuleCode::S100, Diverges(Divergence::AsyncReturnAlias)),
            Self::AsyncReturnMissingArgument => (RuleCode::S100, TscRejects),
            Self::AsyncReturnArgumentCount => (RuleCode::S100, TscRejects),
            Self::TupleAnnotation => (RuleCode::S100, Diverges(Divergence::TupleAnnotation)),
            Self::ThisAnnotation => (RuleCode::S100, Diverges(Divergence::ThisAnnotation)),
            Self::QueryAnnotation => (RuleCode::S100, Diverges(Divergence::QueryAnnotation)),
            Self::StructuralAnnotation => {
                (RuleCode::S100, Diverges(Divergence::StructuralAnnotation))
            }
            Self::OptionalAnnotation => (RuleCode::S100, TscRejects),
            Self::RestAnnotation => (RuleCode::S100, TscRejects),
            Self::ConditionalAnnotation => {
                (RuleCode::S100, Diverges(Divergence::ConditionalAnnotation))
            }
            Self::InferAnnotation => (RuleCode::S100, TscRejects),
            Self::OperatorAnnotation => (RuleCode::S100, Diverges(Divergence::OperatorAnnotation)),
            Self::IndexedAnnotation => (RuleCode::S100, Diverges(Divergence::IndexedAnnotation)),
            Self::MappedAnnotation => (RuleCode::S100, Diverges(Divergence::MappedAnnotation)),
            Self::PredicateAnnotation => {
                (RuleCode::S100, Diverges(Divergence::PredicateAnnotation))
            }
            Self::ImportAnnotation => (RuleCode::S100, Diverges(Divergence::ImportAnnotation)),
            Self::StringLiteralAnnotation => (
                RuleCode::S100,
                Diverges(Divergence::StringLiteralAnnotation),
            ),
            Self::NumberLiteralAnnotation => (
                RuleCode::S100,
                Diverges(Divergence::NumberLiteralAnnotation),
            ),
            Self::BooleanLiteralAnnotation => (
                RuleCode::S100,
                Diverges(Divergence::BooleanLiteralAnnotation),
            ),
            Self::BigIntLiteralAnnotation => (
                RuleCode::S100,
                Diverges(Divergence::BigIntLiteralAnnotation),
            ),
            Self::TemplateLiteralAnnotation => (
                RuleCode::S100,
                Diverges(Divergence::TemplateLiteralAnnotation),
            ),
            Self::UnsupportedAnnotationKind => (RuleCode::S100, TscRejects),
            Self::VoidTypeOutsideResult => (RuleCode::S100, Diverges(Divergence::VoidValue)),
            Self::AnyTypeAnnotation => (RuleCode::S007, Diverges(Divergence::BareNumber)),
            Self::UnknownTypeAnnotation => (RuleCode::S001, Diverges(Divergence::AnyType)),
            Self::UndefinedKeywordAnnotation => (
                RuleCode::S012,
                Diverges(Divergence::GeneralUnionAndUndefined),
            ),
            Self::ObjectTypeOutsideBoundary => {
                (RuleCode::S011, Diverges(Divergence::BoundaryOnlyObject))
            }
            Self::NeverAnnotation => (RuleCode::S100, Diverges(Divergence::NeverAnnotation)),
            Self::UnknownAnnotation => (RuleCode::S100, Diverges(Divergence::UnknownAnnotation)),
            Self::SymbolAnnotation => (RuleCode::S100, Diverges(Divergence::SymbolAnnotation)),
            Self::BigIntAnnotation => (RuleCode::S100, Diverges(Divergence::BigIntAnnotation)),
            Self::UnsupportedKeywordKind => (RuleCode::S100, TscRejects),
            Self::QualifiedSourceTypeName => (
                RuleCode::S100,
                Diverges(Divergence::QualifiedSourceTypeNameForm),
            ),
            Self::WorkerTypeArgumentsMissing => (RuleCode::S100, TscRejects),
            Self::WorkerTypeArgumentCount => (RuleCode::S100, TscRejects),
            Self::WorkerMessagePlainClass => (
                RuleCode::S100,
                Diverges(Divergence::WorkerMessagePlainClass),
            ),
            Self::RegExpTypeArguments => (RuleCode::S100, TscRejects),
            Self::PromiseTypeArgumentMissing => (RuleCode::S100, TscRejects),
            Self::PromiseTypeArgumentCount => (RuleCode::S100, TscRejects),
            Self::FixedArrayTypeArgumentsMissing => (RuleCode::S100, TscRejects),
            Self::FixedArrayTypeArgumentCount => (RuleCode::S100, TscRejects),
            Self::FixedArrayLengthRange => {
                (RuleCode::S008, Diverges(Divergence::FixedArrayLengthRange))
            }
            Self::FixedArrayLengthLiteral => (
                RuleCode::S100,
                Diverges(Divergence::FixedArrayLengthLiteral),
            ),
            Self::FixedArrayByteLimit => {
                (RuleCode::S100, Diverges(Divergence::AggregateLayoutLimit))
            }
            Self::ArrayTypeArgumentCount => {
                (RuleCode::S100, Diverges(Divergence::ArrayTypeArgument))
            }
            Self::GeneratorYieldTypeMissing => (
                RuleCode::S100,
                Diverges(Divergence::GeneratorYieldTypeMissingForm),
            ),
            Self::ErrorTypeArguments => (RuleCode::S100, TscRejects),
            Self::MapTypeArgumentsMissing => (RuleCode::S100, TscRejects),
            Self::MapTypeArgumentCount => (RuleCode::S100, TscRejects),
            Self::DateTypeArguments => (RuleCode::S100, TscRejects),
            Self::NonGenericClassTypeArguments => (RuleCode::S100, TscRejects),
            Self::GenericClassTypeArgumentsMissing => (RuleCode::S100, TscRejects),
            Self::BoundaryLiteralAlias => {
                (RuleCode::S100, Diverges(Divergence::BoundaryLiteralAlias))
            }
            Self::LiteralAliasTypeArguments => (RuleCode::S100, TscRejects),
            Self::UnboundTypeName => (RuleCode::S016, TscRejects),
            Self::TypeNameUnknown => (RuleCode::S016, Diverges(Divergence::LibTypeName)),
            Self::IntersectionTypeAnnotation => (
                RuleCode::S100,
                Diverges(Divergence::IntersectionTypeAnnotationForm),
            ),
            Self::UndefinedUnionMember => (
                RuleCode::S012,
                Diverges(Divergence::GeneralUnionAndUndefined),
            ),
            Self::NullableNonReference => {
                (RuleCode::S011, Diverges(Divergence::NullableNonReference))
            }
            Self::ConstructorTypeAnnotation => (
                RuleCode::S100,
                Diverges(Divergence::ConstructorTypeAnnotationForm),
            ),
            Self::FunctionTypeParameterAnnotationMissing => (RuleCode::S100, TscRejects),
            Self::FunctionTypeRestParameter => (
                RuleCode::S100,
                Diverges(Divergence::FunctionTypeRestParameter),
            ),
            Self::FunctionTypeArrayPattern => (
                RuleCode::S100,
                Diverges(Divergence::FunctionTypeArrayPattern),
            ),
            Self::FunctionTypeObjectPattern => (
                RuleCode::S100,
                Diverges(Divergence::FunctionTypeObjectPattern),
            ),
            Self::NamespaceUnexportedMember => (
                RuleCode::S016,
                Diverges(Divergence::NamespaceUnexportedMember),
            ),
            Self::IncompatibleNominalAssignment => (RuleCode::S005, TscRejects),
            Self::ErasedNominalTypeArguments => (
                RuleCode::S005,
                Diverges(Divergence::ErasedAssignableTypeMismatch),
            ),
            Self::AbstractMethodBodyMissing => (
                RuleCode::S100,
                Diverges(Divergence::AbstractMethodBodyMissing),
            ),
            Self::ThisStaticMethodMember => {
                (RuleCode::S100, Diverges(Divergence::ThisStaticMethodMember))
            }
            Self::InstanceMemberInStaticMethod => (RuleCode::S100, TscRejects),
            Self::DistinctNominalClassAssignment => {
                (RuleCode::S005, Diverges(Divergence::NominalClassIdentity))
            }
            Self::NullableNominalAssignment => (RuleCode::S005, TscRejects),
            Self::NullableNominalAssignmentNonNullFlow => (
                RuleCode::S005,
                Diverges(Divergence::NullableNominalAssignmentNonNullFlow),
            ),
            Self::DistinctNominalContainerAssignment => (
                RuleCode::S005,
                Diverges(Divergence::DistinctNominalContainerAssignmentForm),
            ),
            Self::ImplicitNumericAssignment => {
                (RuleCode::S007, Diverges(Divergence::SizedOperandWidths))
            }
            Self::NullableValueClassAssignment => (
                RuleCode::S011,
                Diverges(Divergence::NullableValueClassAssignment),
            ),
            Self::LiteralAliasToString => {
                (RuleCode::S100, Diverges(Divergence::LiteralAliasToString))
            }
            Self::EnumToInteger => (RuleCode::S100, Diverges(Divergence::EnumToInteger)),
            Self::ArrayToFixedArray => (RuleCode::S100, Diverges(Divergence::ArrayToFixedArray)),
            Self::FunctionParameterIdentity => (
                RuleCode::S100,
                Diverges(Divergence::FunctionParameterIdentity),
            ),
            Self::AssignmentTypeMismatch => (RuleCode::S100, TscRejects),
            Self::CaptureEffectEscapes => (RuleCode::S009, Diverges(Divergence::EscapingCapture)),
            Self::CaptureEffectArgument => (RuleCode::S009, Diverges(Divergence::EscapingCapture)),
            Self::UnaryNumericCoercion => {
                (RuleCode::S100, Diverges(Divergence::UnaryNumericCoercion))
            }
            Self::LogicalNotNonBoolean => (
                RuleCode::S100,
                Diverges(Divergence::LogicalNotNonBooleanForm),
            ),
            Self::BitwiseIntegerOperand => {
                (RuleCode::S100, Diverges(Divergence::BitwiseIntegerOperand))
            }
            Self::DeleteProperty => (RuleCode::S100, Diverges(Divergence::DeleteProperty)),
            Self::TypeofOperator => (RuleCode::S100, Diverges(Divergence::TypeofOperator)),
            Self::VoidOperator => (RuleCode::S100, Diverges(Divergence::VoidOperator)),
            Self::UnaryPlusOperator => (RuleCode::S100, Diverges(Divergence::UnaryPlusOperator)),
            Self::IndexUpdateExpressionValue => {
                (RuleCode::S100, Diverges(Divergence::ClassIndexSignature))
            }
            Self::AccessorUpdateExpressionValue => {
                (RuleCode::S100, Diverges(Divergence::NamedAccessor))
            }
            Self::UpdateNonNumericTarget => (RuleCode::S100, TscRejects),
            Self::ReadonlyIndexUpdate => (RuleCode::S100, TscRejects),
            Self::ReadonlyAccessorUpdate => (RuleCode::S100, TscRejects),
            Self::UpdateSetterParameterMissing => (RuleCode::S100, TscRejects),
            Self::LogicalNonBooleanOperand => (
                RuleCode::S100,
                Diverges(Divergence::LogicalNonBooleanOperandForm),
            ),
            Self::InOperator => (RuleCode::S100, Diverges(Divergence::InOperator)),
            Self::ExponentOperator => (RuleCode::S100, Diverges(Divergence::ExponentOperator)),
            Self::OptionalCallValueWithoutFallback => {
                (RuleCode::S012, Diverges(Divergence::OptionalChainUnbound))
            }
            Self::OptionalMemberValueWithoutFallback => {
                (RuleCode::S012, Diverges(Divergence::OptionalChainUnbound))
            }
            Self::OptionalComputedMember => {
                (RuleCode::S100, Diverges(Divergence::OptionalChainIndex))
            }
            Self::OptionalMethodCall => (RuleCode::S100, Diverges(Divergence::OptionalMethodCall)),
            Self::OptionalFunctionCall => {
                (RuleCode::S100, Diverges(Divergence::OptionalFunctionCall))
            }
            Self::OptionalPrivateMember => (RuleCode::S100, TscRejects),
            Self::UndefinedEqualityPair => {
                (RuleCode::S012, Diverges(Divergence::UndefinedEqualityPair))
            }
            Self::UndefinedEqualityNonMember => (
                RuleCode::S012,
                Diverges(Divergence::UndefinedEqualityNonMember),
            ),
            Self::CompoundEnumOperand => {
                (RuleCode::S100, Diverges(Divergence::CompoundEnumOperand))
            }
            Self::CompoundStringOperand => {
                (RuleCode::S100, Diverges(Divergence::CompoundStringOperand))
            }
            Self::CompoundInvalidOperand => (RuleCode::S100, TscRejects),
            Self::BinaryMixedNumericTypes => {
                (RuleCode::S007, Diverges(Divergence::SizedOperandWidths))
            }
            Self::BinaryEnumOperand => (RuleCode::S100, Diverges(Divergence::BinaryEnumOperand)),
            Self::StringRelationalOperand => (
                RuleCode::S100,
                Diverges(Divergence::StringRelationalOperand),
            ),
            Self::BinaryStringOperand => {
                (RuleCode::S100, Diverges(Divergence::BinaryStringOperand))
            }
            Self::BooleanRelationalOperand => (
                RuleCode::S100,
                Diverges(Divergence::BooleanRelationalOperand),
            ),
            Self::BinaryInvalidOperand => (RuleCode::S100, TscRejects),
            Self::ConditionalNonBooleanCondition => (
                RuleCode::S100,
                Diverges(Divergence::ConditionalNonBooleanConditionForm),
            ),
            Self::AsyncGeneratorYield => {
                (RuleCode::S100, Diverges(Divergence::AsyncGeneratorFunction))
            }
            Self::YieldDelegation => (RuleCode::S100, Diverges(Divergence::YieldDelegationForm)),
            Self::BareYieldNonVoid => (RuleCode::S100, Diverges(Divergence::BareYieldNonVoid)),
            Self::IdentityAssertion => (RuleCode::S100, Diverges(Divergence::IdentityAssertion)),
            Self::IntegerEnumAssertion => {
                (RuleCode::S100, Diverges(Divergence::IntegerEnumAssertion))
            }
            Self::NullableClassAssertion => {
                (RuleCode::S100, Diverges(Divergence::NullableClassAssertion))
            }
            Self::StringAliasAssertion => {
                (RuleCode::S100, Diverges(Divergence::StringAliasAssertion))
            }
            Self::InvalidAssertion => (RuleCode::S100, TscRejects),
            Self::DescriptorDefaultThisArithmetic => {
                (RuleCode::S100, Diverges(Divergence::ThisInFieldInitializer))
            }
            Self::FieldInitializerUninitializedRead => {
                (RuleCode::S100, Diverges(Divergence::ThisInFieldInitializer))
            }
            Self::FieldInitializerSelfRead => {
                (RuleCode::S100, Diverges(Divergence::ThisInFieldInitializer))
            }
            Self::DescriptorAbsentMemberRead => (
                RuleCode::S100,
                Diverges(Divergence::OptionalDescriptorMember),
            ),
            Self::PrivateMemberRead => {
                (RuleCode::S100, Diverges(Divergence::PrivateMemberReadForm))
            }
            Self::ArrayIndexNotInt => (RuleCode::S100, Diverges(Divergence::ArrayIndexNotIntForm)),
            Self::FixedArrayIndexNotInt => (
                RuleCode::S100,
                Diverges(Divergence::FixedArrayIndexNotIntForm),
            ),
            Self::FixedArrayConstantIndexBounds => (
                RuleCode::S100,
                Diverges(Divergence::FixedArrayConstantIndexBoundsForm),
            ),
            Self::NonIndexableReceiver => (
                RuleCode::S100,
                Diverges(Divergence::NonIndexableReceiverForm),
            ),
            Self::ValueClassMemberMissing => (RuleCode::S018, TscRejects),
            Self::InstancePrototypeMember => {
                (RuleCode::S003, Diverges(Divergence::DynamicObjectModel))
            }
            Self::ReadSetterOnlyAccessor => (
                RuleCode::S100,
                Diverges(Divergence::WriteAccessorWithoutRead),
            ),
            Self::InstanceStaticMemberRead => (RuleCode::S100, TscRejects),
            Self::ClassObjectMemberWrite => (
                RuleCode::S004,
                Diverges(Divergence::ClassInheritedObjectMember),
            ),
            Self::AsyncMethodValue => (RuleCode::S100, Diverges(Divergence::AsyncMethodValue)),
            Self::GenericAsyncMethodValue => (
                RuleCode::S100,
                Diverges(Divergence::GenericAsyncMethodValue),
            ),
            Self::ClassObjectMemberRead => (
                RuleCode::S018,
                Diverges(Divergence::ClassInheritedObjectMember),
            ),
            Self::ArrayMethodValue => (RuleCode::S100, Diverges(Divergence::ArrayMethodValueForm)),
            Self::FixedArrayMethodValue => (
                RuleCode::S100,
                Diverges(Divergence::FixedArrayMethodValueForm),
            ),
            Self::FixedArrayObjectMember => {
                (RuleCode::S100, Diverges(Divergence::FixedArrayObjectMember))
            }
            Self::MapMethodValue => (RuleCode::S100, Diverges(Divergence::MapMethodValueForm)),
            Self::MapObjectMember => (RuleCode::S100, Diverges(Divergence::MapObjectMember)),
            Self::SetMethodValue => (RuleCode::S100, Diverges(Divergence::SetMethodValueForm)),
            Self::SetObjectMember => (RuleCode::S100, Diverges(Divergence::SetObjectMember)),
            Self::StringMethodValue => {
                (RuleCode::S100, Diverges(Divergence::StringMethodValueForm))
            }
            Self::GeneratorResultDoneWrite => (
                RuleCode::S100,
                Diverges(Divergence::GeneratorResultDoneWriteForm),
            ),
            Self::GeneratorResultValueWrite => (
                RuleCode::S100,
                Diverges(Divergence::GeneratorResultValueWriteForm),
            ),
            Self::GeneratorResultObjectMember => (
                RuleCode::S100,
                Diverges(Divergence::GeneratorResultObjectMember),
            ),
            Self::NumericObjectMember => {
                (RuleCode::S018, Diverges(Divergence::NumericObjectMember))
            }
            Self::BoundaryObjectMember => {
                (RuleCode::S100, Diverges(Divergence::BoundaryObjectMember))
            }
            Self::BooleanMember => (RuleCode::S100, Diverges(Divergence::BooleanMember)),
            Self::FunctionMember => (RuleCode::S100, Diverges(Divergence::FunctionMember)),
            Self::GeneratorMember => (RuleCode::S100, Diverges(Divergence::GeneratorMember)),
            Self::EnumMember => (RuleCode::S100, Diverges(Divergence::EnumMember)),
            Self::LiteralAliasMember => (RuleCode::S100, Diverges(Divergence::LiteralAliasMember)),
            Self::InvalidReceiverMember => (RuleCode::S100, TscRejects),
            Self::ArrayOfTypeArgumentCount => (RuleCode::S100, TscRejects),
            Self::MapCopyTypeArgumentCount => (RuleCode::S100, TscRejects),
            Self::MapCopyNullableSource => {
                (RuleCode::S011, Diverges(Divergence::MapCopyNullableSource))
            }
            Self::AsyncArrowFunction => (RuleCode::S100, Diverges(Divergence::AsyncFunctionShape)),
            Self::GeneratorArrowFunction => (RuleCode::S100, TscRejects),
            Self::BlockLambdaReturnAnnotationMissing => (
                RuleCode::S100,
                Diverges(Divergence::BlockLambdaReturnAnnotationMissingForm),
            ),
            Self::LambdaReturnCoverage => (
                RuleCode::S100,
                Diverges(Divergence::LambdaReturnFlowCoverage),
            ),
            Self::RegexLiteralUnicodeSetsFlag => (RuleCode::S100, TscRejects),
            Self::BigIntLiteral => (RuleCode::S100, Diverges(Divergence::BigIntLiteral)),
            Self::HalfFloatLiteralRange => {
                (RuleCode::S008, Diverges(Divergence::IntegerLiteralRange))
            }
            Self::FractionalIntegerLiteral => (
                RuleCode::S008,
                Diverges(Divergence::FractionalIntegerLiteral),
            ),
            Self::IntegerLiteralRange => {
                (RuleCode::S008, Diverges(Divergence::IntegerLiteralRange))
            }
            Self::TemplateInterpolationKind => (
                RuleCode::S100,
                Diverges(Divergence::TemplateInterpolationKindForm),
            ),
            Self::UnknownValueName => (RuleCode::S100, Diverges(Divergence::NamedModuleSurface)),
            Self::NamespaceContextValue => (RuleCode::S100, Diverges(Divergence::NamespaceAsValue)),
            Self::AsyncFunctionValue => (RuleCode::S100, Diverges(Divergence::AsyncFunctionValue)),
            Self::GenericFunctionValue => {
                (RuleCode::S100, Diverges(Divergence::GenericFunctionValue))
            }
            Self::ClassRuntimeValue => (RuleCode::S100, Diverges(Divergence::ClassRuntimeObject)),
            Self::EnumObjectValue => (RuleCode::S100, Diverges(Divergence::EnumObjectValueForm)),
            Self::MirrorTypeAliasValue => (RuleCode::S100, TscRejects),
            Self::LiteralAliasValue => (RuleCode::S100, TscRejects),
            Self::ForeignFunctionValue => (
                RuleCode::S100,
                Diverges(Divergence::ForeignFunctionValueForm),
            ),
            Self::DynamicEvaluatorValue => {
                (RuleCode::S002, Diverges(Divergence::DynamicObjectModel))
            }
            Self::AmbientFunctionValue => (
                RuleCode::S100,
                Diverges(Divergence::AmbientFunctionValueForm),
            ),
            Self::UnboundValueName => (RuleCode::S016, TscRejects),
            Self::UnknownAmbientValueName => (RuleCode::S016, Diverges(Divergence::LibGlobalValue)),
            Self::ArrayLiteralHole => (RuleCode::S100, Diverges(Divergence::ArrayHoleConstruction)),
            Self::FixedArrayLiteralLength => (
                RuleCode::S100,
                Diverges(Divergence::FixedArrayLiteralLength),
            ),
            Self::EmptyArrayInference => (
                RuleCode::S100,
                Diverges(Divergence::EmptyArrayInferenceForm),
            ),
            Self::DescriptorLiteralSpread => (
                RuleCode::S100,
                Diverges(Divergence::DescriptorLiteralSpreadForm),
            ),
            Self::DescriptorLiteralQuotedKey => (
                RuleCode::S100,
                Diverges(Divergence::DescriptorLiteralQuotedKeyForm),
            ),
            Self::DescriptorLiteralAccessor => (
                RuleCode::S100,
                Diverges(Divergence::DescriptorLiteralAccessorForm),
            ),
            Self::DescriptorLiteralDuplicateMember => (RuleCode::S100, TscRejects),
            Self::DescriptorLiteralUnknownMember => (RuleCode::S004, TscRejects),
            Self::DescriptorRequiredMemberQuotedKey => (RuleCode::S100, TscRejects),
            Self::ArraySpreadLiteralHole => {
                (RuleCode::S100, Diverges(Divergence::ArrayHoleConstruction))
            }
            Self::NonExpressionCalleeUnavailable => (RuleCode::S100, TscRejects),
            Self::SuperConstructorCall => (
                RuleCode::S100,
                Diverges(Divergence::ReferenceClassInheritance),
            ),
            Self::DynamicImportCall => (RuleCode::S100, Diverges(Divergence::DynamicImportCall)),
            Self::NonGenericFunctionTypeArguments => (RuleCode::S100, TscRejects),
            Self::ClassCalledWithoutNew => (RuleCode::S100, TscRejects),
            Self::EnumCalled => (RuleCode::S100, TscRejects),
            Self::MirrorTypeAliasCalled => (RuleCode::S100, TscRejects),
            Self::LiteralAliasCalled => (RuleCode::S100, TscRejects),
            Self::DynamicEvaluatorCalled => {
                (RuleCode::S002, Diverges(Divergence::DynamicObjectModel))
            }
            Self::UnreachableExpressionValue => (
                RuleCode::S100,
                Diverges(Divergence::UnreachableInValuePosition),
            ),
            Self::UnboundFunctionName => (RuleCode::S016, TscRejects),
            Self::UnknownFunctionName => (RuleCode::S016, Diverges(Divergence::LibGlobalCall)),
            Self::GeneratorYieldTypeNotKnown => (
                RuleCode::S100,
                Diverges(Divergence::GeneratorYieldTypeNotKnownForm),
            ),
            Self::ContextByteTargetKind => (RuleCode::S100, Diverges(Divergence::ByteAccessTarget)),
            Self::ContextByteTargetLayout => {
                (RuleCode::S100, Diverges(Divergence::ByteAccessTarget))
            }
            Self::ByteArgumentIdentity => {
                (RuleCode::S100, Diverges(Divergence::ByteArgumentIdentity))
            }
            Self::NonGenericStaticMethodTypeArguments => (RuleCode::S100, TscRejects),
            Self::PromiseCombinatorCall => (RuleCode::S013, Diverges(Divergence::PromiseObject)),
            Self::PromiseStaticCall => (RuleCode::S013, Diverges(Divergence::PromiseObject)),
            Self::WorkerStaticObjectMethod => (
                RuleCode::S018,
                Diverges(Divergence::WorkerStaticObjectMethod),
            ),
            Self::GenericMethodTypeArgumentsMissing => (
                RuleCode::S100,
                Diverges(Divergence::GenericMethodTypeArguments),
            ),
            Self::ValueClassMethodMissing => (RuleCode::S018, TscRejects),
            Self::ToStringTypeArguments => (RuleCode::S100, TscRejects),
            Self::WorkerObjectMethod => (RuleCode::S018, Diverges(Divergence::WorkerObjectMethod)),
            Self::InboxObjectMethod => (RuleCode::S018, Diverges(Divergence::InboxObjectMethod)),
            Self::OutboxObjectMethod => (RuleCode::S018, Diverges(Divergence::OutboxObjectMethod)),
            Self::ToStringArgumentCount => (RuleCode::S100, TscRejects),
            Self::FixedArrayObjectMethod => {
                (RuleCode::S018, Diverges(Divergence::FixedArrayObjectMethod))
            }
            Self::CoroutineStepLayoutLimit => {
                (RuleCode::S100, Diverges(Divergence::AggregateLayoutLimit))
            }
            Self::CoroutineReturnOrThrowCall => (
                RuleCode::S100,
                Diverges(Divergence::CoroutineReturnOrThrowCallForm),
            ),
            Self::InstanceStaticMethodCall => (RuleCode::S100, TscRejects),
            Self::ClassObjectMethodCall => (
                RuleCode::S018,
                Diverges(Divergence::ClassInheritedObjectMember),
            ),
            Self::BooleanMethod => (RuleCode::S100, Diverges(Divergence::BooleanMethod)),
            Self::FunctionMethod => (RuleCode::S100, Diverges(Divergence::FunctionMethod)),
            Self::GeneratorMethod => (RuleCode::S100, TscRejects),
            Self::EnumMethod => (RuleCode::S100, Diverges(Divergence::EnumMethod)),
            Self::LiteralAliasMethod => (RuleCode::S100, Diverges(Divergence::LiteralAliasMethod)),
            Self::InvalidReceiverMethod => (RuleCode::S100, TscRejects),
            Self::ReferenceConstructorArgumentCount => (RuleCode::S100, TscRejects),
            Self::ValueConstructorArgumentCount => (RuleCode::S100, TscRejects),
            Self::InstanceMethodArgumentCount => (RuleCode::S100, TscRejects),
            Self::AsyncMethodArgumentCount => (RuleCode::S100, TscRejects),
            Self::GeneratorNextArgumentCount => (
                RuleCode::S100,
                Diverges(Divergence::GeneratorNextArgumentCount),
            ),
            Self::ArrayPopArgumentCount => (RuleCode::S100, TscRejects),
            Self::ArrayPushArgumentCount => {
                (RuleCode::S100, Diverges(Divergence::ArrayPushArgumentCount))
            }
            Self::FixedArrayPushArgumentCount => (RuleCode::S100, TscRejects),
            Self::OutboxPostArgumentCount => (RuleCode::S100, TscRejects),
            Self::InboxWaitArgumentCount => (RuleCode::S100, TscRejects),
            Self::ScalarToStringArgumentCount => (RuleCode::S100, TscRejects),
            Self::FunctionValueArgumentCount => (RuleCode::S100, TscRejects),
            Self::AmbientFunctionArgumentCount => (RuleCode::S100, TscRejects),
            Self::ContextMethodArgumentCount => (RuleCode::S100, TscRejects),
            Self::ForeignFunctionArgumentCount => (RuleCode::S100, TscRejects),
            Self::SourceFunctionArgumentCount => (RuleCode::S100, TscRejects),
            Self::AwaitFunctionArgumentCount => (RuleCode::S100, TscRejects),
            Self::AwaitMethodArgumentCount => (RuleCode::S100, TscRejects),
            Self::SetBinaryArgumentCount => (RuleCode::S100, TscRejects),
            Self::SetDeleteArgumentCount => (RuleCode::S100, TscRejects),
            Self::SetClearArgumentCount => (RuleCode::S100, TscRejects),
            Self::SetAddOrHasArgumentCount => (RuleCode::S100, TscRejects),
            Self::MapClearArgumentCount => (RuleCode::S100, TscRejects),
            Self::MapHasArgumentCount => (RuleCode::S100, TscRejects),
            Self::MapSetArgumentCount => (RuleCode::S100, TscRejects),
            Self::MapGetOrArgumentCount => (RuleCode::S100, TscRejects),
            Self::MapGetArgumentCount => (RuleCode::S100, TscRejects),
            Self::ArrayCopyWithinArgumentCount => (RuleCode::S100, TscRejects),
            Self::ArrayUnshiftArgumentCount => (RuleCode::S100, TscRejects),
            Self::ArrayShiftArgumentCount => (RuleCode::S100, TscRejects),
            Self::ArraySpliceArgumentCount => (RuleCode::S100, TscRejects),
            Self::ArrayConcatCallArgumentCount => (
                RuleCode::S100,
                Diverges(Divergence::ArrayConcatArgumentCount),
            ),
            Self::ArrayReverseArgumentCount => (RuleCode::S100, TscRejects),
            Self::ArrayFillArgumentCount => (RuleCode::S100, TscRejects),
            Self::ArraySliceArgumentCount => (RuleCode::S100, TscRejects),
            Self::ArrayJoinArgumentCount => (RuleCode::S100, TscRejects),
            Self::ArraySearchArgumentCount => (RuleCode::S100, TscRejects),
            Self::ArrayAtArgumentCount => (RuleCode::S100, TscRejects),
            Self::StringConcatArgumentCount => (
                RuleCode::S100,
                Diverges(Divergence::StringConcatArgumentCount),
            ),
            Self::FixedArrayAtArgumentCount => (RuleCode::S100, TscRejects),
            Self::NumericMethodCheckedArgumentCount => (RuleCode::S100, TscRejects),
            Self::BigIntConversionArgumentCount => (RuleCode::S100, TscRejects),
            Self::NumberConversionArgumentCount => (RuleCode::S100, TscRejects),
            Self::DateNowArgumentCount => (RuleCode::S100, TscRejects),
            Self::DateUtcArgumentCount => (RuleCode::S100, TscRejects),
            Self::RegexExecArgumentCount => (RuleCode::S100, TscRejects),
            Self::RegexTestArgumentCount => (RuleCode::S100, TscRejects),
            Self::RegexConstructorArgumentCount => (RuleCode::S100, TscRejects),
            Self::GlobalNumberArgumentCount => (RuleCode::S100, TscRejects),
            Self::NumberPredicateArgumentCount => (RuleCode::S100, TscRejects),
            Self::MathMethodArgumentCount => (RuleCode::S100, TscRejects),
            Self::UriCodecArgumentCount => (RuleCode::S100, TscRejects),
            Self::ConstructorNotNamedClass => (
                RuleCode::S100,
                Diverges(Divergence::ConstructorNotNamedClassForm),
            ),
            Self::LocalValueConstructed => (RuleCode::S100, TscRejects),
            Self::DynamicFunctionConstructed => {
                (RuleCode::S002, Diverges(Divergence::DynamicObjectModel))
            }
            Self::PromiseConstructed => (RuleCode::S013, Diverges(Divergence::PromiseObject)),
            Self::WorkerEndpointConstructed => (RuleCode::S100, TscRejects),
            Self::ContainerConstructorTypeArgumentsMissing => (
                RuleCode::S100,
                Diverges(Divergence::GenericConstructorTypeArguments),
            ),
            Self::ContainerConstructorTypeArgumentCount => (RuleCode::S100, TscRejects),
            Self::SetConstructorSourceCount => (RuleCode::S100, TscRejects),
            Self::NonGenericConstructorTypeArguments => (RuleCode::S100, TscRejects),
            Self::GenericConstructorTypeArguments => (
                RuleCode::S100,
                Diverges(Divergence::GenericConstructorTypeArguments),
            ),
            Self::OpaqueHandleConstructed => (RuleCode::S100, TscRejects),
            Self::AmbientClassConstructed => (
                RuleCode::S100,
                Diverges(Divergence::AmbientClassConstruction),
            ),
            Self::DescriptorClassConstructed => {
                (RuleCode::S100, Diverges(Divergence::DescriptorConstruction))
            }
            Self::NamespaceClassPrototypeRead => {
                (RuleCode::S003, Diverges(Divergence::DynamicObjectModel))
            }
            Self::NamespaceConstFieldWrite => (RuleCode::S100, TscRejects),
            Self::ReadStaticSetterOnlyAccessor => (
                RuleCode::S018,
                Diverges(Divergence::WriteAccessorWithoutRead),
            ),
            Self::StaticMethodValue => {
                (RuleCode::S100, Diverges(Divergence::StaticMethodValueForm))
            }
            Self::ClassRuntimeMember => (RuleCode::S018, Diverges(Divergence::ClassRuntimeObject)),
            Self::NamespacePrototypeRead => {
                (RuleCode::S003, Diverges(Divergence::DynamicObjectModel))
            }
            Self::GenericClassRuntimeMember => {
                (RuleCode::S018, Diverges(Divergence::ClassRuntimeObject))
            }
            Self::EnumStaticMemberMissing => (RuleCode::S018, TscRejects),
            Self::MirrorAliasStaticValue => (RuleCode::S100, TscRejects),
            Self::LiteralAliasStaticValue => (RuleCode::S100, TscRejects),
            Self::GlobalPrototypeRead => (RuleCode::S003, Diverges(Divergence::DynamicObjectModel)),
            Self::NumberParserIdentityMismatch => (RuleCode::S100, TscRejects),
            Self::WorkerMessageNotTransferable => {
                (RuleCode::S100, Diverges(Divergence::WorkerContextAffinity))
            }
            Self::WorkerSpawnSpread => (RuleCode::S100, Diverges(Divergence::WorkerSpawnSpread)),
            Self::WorkerEntryNotNamedFunction => {
                (RuleCode::S100, Diverges(Divergence::WorkerEntryShape))
            }
            Self::WorkerEntryLocalValue => {
                (RuleCode::S100, Diverges(Divergence::WorkerEntryLocalValue))
            }
            Self::WorkerEntryGeneric => (RuleCode::S100, Diverges(Divergence::WorkerEntryGeneric)),
            Self::WorkerEntryAsync => (RuleCode::S100, Diverges(Divergence::WorkerEntryShape)),
            Self::WorkerEntryShape => (RuleCode::S100, Diverges(Divergence::WorkerEntrySignature)),
            Self::WorkerEntryStructuralEndpoints => (
                RuleCode::S100,
                Diverges(Divergence::WorkerEntryStructuralEndpoints),
            ),
            Self::WorkerSpawnTypeArgumentCount => (RuleCode::S100, TscRejects),
            Self::WorkerExplicitMessageIdentity => (
                RuleCode::S100,
                Diverges(Divergence::WorkerExplicitMessageIdentity),
            ),
            Self::RegexConstructorTypeArguments => (RuleCode::S100, TscRejects),
            Self::RegexCompile => (RuleCode::S100, Diverges(Divergence::RegexCompile)),
            Self::ReplaceAllRegexNotGlobal => {
                (RuleCode::S100, Diverges(Divergence::ReplaceAllGlobalFlag))
            }
            Self::StringStaticMethodArgumentCount => (RuleCode::S100, TscRejects),
            Self::VoidExpressionValue => (RuleCode::S100, Diverges(Divergence::VoidValue)),
            Self::DescriptorDefaultThisUse => {
                (RuleCode::S100, Diverges(Divergence::ThisInFieldInitializer))
            }
            Self::FieldInitializerThisUse => {
                (RuleCode::S100, Diverges(Divergence::ThisInFieldInitializer))
            }
            Self::ThisOutsideMethod => (RuleCode::S100, TscRejects),
            Self::NominalObjectLiteral => (
                RuleCode::S005,
                Diverges(Divergence::ObjectLiteralConstruction),
            ),
            Self::ObjectLiteralWithoutDescriptorContext => (
                RuleCode::S100,
                Diverges(Divergence::ObjectLiteralConstruction),
            ),
            Self::NonNullAssertionExpression => (
                RuleCode::S100,
                Diverges(Divergence::NonNullAssertionExpressionForm),
            ),
            Self::FunctionExpression => {
                (RuleCode::S100, Diverges(Divergence::FunctionExpressionForm))
            }
            Self::AngleAssertionExpression => (
                RuleCode::S100,
                Diverges(Divergence::AngleAssertionExpression),
            ),
            Self::SatisfiesExpression => {
                (RuleCode::S100, Diverges(Divergence::SatisfiesExpression))
            }
            Self::InstantiationExpression => (
                RuleCode::S100,
                Diverges(Divergence::InstantiationExpression),
            ),
            Self::CommaExpression => (RuleCode::S100, Diverges(Divergence::CommaExpression)),
            Self::TaggedTemplateExpression => (
                RuleCode::S100,
                Diverges(Divergence::TaggedTemplateExpression),
            ),
            Self::ClassExpression => (RuleCode::S100, Diverges(Divergence::ClassExpression)),
            Self::MetaPropertyExpression => {
                (RuleCode::S100, Diverges(Divergence::MetaPropertyExpression))
            }
            Self::PrivateNameExpression => (RuleCode::S100, TscRejects),
            // C24 row 18 decides const assertions.
            Self::ConstAssertionExpression => (
                RuleCode::S100,
                Diverges(Divergence::ConstAssertionExpression),
            ),
            Self::UnsupportedExpressionKind => (RuleCode::S100, TscRejects),
            Self::EmbeddedHeaderCopied => {
                (RuleCode::S100, Diverges(Divergence::EmbeddedHeaderCopy))
            }
            Self::AwaitOutsideAsync => (RuleCode::S013, Diverges(Divergence::AwaitOutsideAsync)),
            Self::AwaitNonHandle => (RuleCode::S100, Diverges(Divergence::AwaitNonHandle)),
            Self::AwaitNotDirectCall => (RuleCode::S100, TscRejects),
            Self::ContextSuspendArguments => (RuleCode::S100, TscRejects),
            Self::AwaitLocalCall => (RuleCode::S100, Diverges(Divergence::AwaitLocalCall)),
            Self::AwaitUndeclaredAsyncFunction => (
                RuleCode::S100,
                Diverges(Divergence::AwaitUndeclaredAsyncFunction),
            ),
            Self::AwaitSynchronousFunction => (
                RuleCode::S100,
                Diverges(Divergence::AwaitSynchronousFunction),
            ),
            Self::AwaitFunctionTypeArguments => (RuleCode::S100, TscRejects),
            Self::AwaitComputedMethod => {
                (RuleCode::S100, Diverges(Divergence::AwaitComputedMethod))
            }
            Self::AwaitNonClassMethod => {
                (RuleCode::S018, Diverges(Divergence::AwaitNonClassMethod))
            }
            Self::AwaitClassObjectMethod => (
                RuleCode::S018,
                Diverges(Divergence::ClassInheritedObjectMember),
            ),
            Self::AwaitSynchronousMethod => {
                (RuleCode::S100, Diverges(Divergence::AwaitSynchronousMethod))
            }
            Self::AwaitMethodTypeArguments => (RuleCode::S100, TscRejects),
            Self::AwaitIndirectCall => (RuleCode::S100, Diverges(Divergence::AwaitIndirectCall)),
            Self::UnsignedShiftAssignment => {
                (RuleCode::S100, Diverges(Divergence::NullishAssignment))
            }
            Self::LogicalOrPowerAssignment => (
                RuleCode::S100,
                Diverges(Divergence::LogicalOrPowerAssignmentForm),
            ),
            Self::DestructuringAssignment => {
                (RuleCode::S100, Diverges(Divergence::AssignmentPattern))
            }
            Self::IndexAssignmentExpressionValue => {
                (RuleCode::S100, Diverges(Divergence::ClassIndexSignature))
            }
            Self::ReadonlyIndexAssignment => (RuleCode::S100, TscRejects),
            Self::StaticAccessorAssignmentExpressionValue => {
                (RuleCode::S100, Diverges(Divergence::NamedAccessor))
            }
            Self::ReadonlyStaticAccessorAssignment => (RuleCode::S100, TscRejects),
            Self::StaticSetterParameterMissing => (RuleCode::S100, TscRejects),
            Self::AccessorAssignmentExpressionValue => {
                (RuleCode::S100, Diverges(Divergence::NamedAccessor))
            }
            Self::ReadonlyAccessorAssignment => (RuleCode::S100, TscRejects),
            Self::SetterParameterMissing => (RuleCode::S100, TscRejects),
            Self::ConstLocalAssignment => (RuleCode::S100, TscRejects),
            Self::ImportedBindingAssignment => (RuleCode::S100, TscRejects),
            Self::ConstGlobalAssignment => (RuleCode::S100, TscRejects),
            Self::NonVariableAssignmentTarget => (RuleCode::S100, TscRejects),
            Self::NonPlaceAssignmentTarget => (
                RuleCode::S100,
                Diverges(Divergence::NonPlaceAssignmentTargetForm),
            ),
            Self::WriteSetterOnlyAccessor => (
                RuleCode::S100,
                Diverges(Divergence::WriteAccessorWithoutRead),
            ),
            Self::PrivateMemberAssignment => (
                RuleCode::S100,
                Diverges(Divergence::PrivateMemberAssignmentForm),
            ),
            Self::ConstStaticFieldAssignment => (RuleCode::S100, TscRejects),
            Self::WriteStaticSetterOnlyAccessor => (
                RuleCode::S018,
                Diverges(Divergence::WriteAccessorWithoutRead),
            ),
            Self::NumericObjectMethod => {
                (RuleCode::S018, Diverges(Divergence::NumericObjectMethod))
            }
            Self::StringObjectMember => (RuleCode::S100, Diverges(Divergence::StringObjectMember)),
            Self::ArraySpliceRequiredArgument => (RuleCode::S100, TscRejects),
            Self::ArrayUnshiftEmpty => (RuleCode::S100, Diverges(Divergence::ArrayUnshiftEmpty)),
            Self::ArrayCopyWithinRequiredArgumentCount => (RuleCode::S100, TscRejects),
            Self::ArraySortArgumentCount => (RuleCode::S100, TscRejects),
            Self::ArrayReduceArgumentCount => (RuleCode::S100, TscRejects),
            Self::ArrayCallbackThisArgument => (
                RuleCode::S100,
                Diverges(Divergence::ArrayCallbackThisArgument),
            ),
            Self::ArrayMapVoidCallback => (RuleCode::S100, Diverges(Divergence::VoidValue)),
            Self::MapGroupByArgumentCount => (RuleCode::S100, TscRejects),
            Self::MapGroupByArraySource => {
                (RuleCode::S100, Diverges(Divergence::MapGroupByArraySource))
            }
            Self::MapGroupByVoidKey => (RuleCode::S100, Diverges(Divergence::MapGroupByVoidKey)),
            Self::MapGroupByNonFunctionCallback => (RuleCode::S100, TscRejects),
            Self::MapObjectMethod => (RuleCode::S100, Diverges(Divergence::MapObjectMethod)),
            Self::MapCallbackThisArgument => (
                RuleCode::S100,
                Diverges(Divergence::MapCallbackThisArgument),
            ),
            Self::SetObjectMethod => (RuleCode::S100, Diverges(Divergence::SetObjectMethod)),
            Self::SetCallbackThisArgument => (
                RuleCode::S100,
                Diverges(Divergence::SetCallbackThisArgument),
            ),
            Self::SetBinaryRequiredArgumentCount => (RuleCode::S100, TscRejects),
            Self::CollectionCallbackTypeMismatch => (
                RuleCode::S100,
                Diverges(Divergence::CollectionCallbackTypeMismatchForm),
            ),
            Self::ArrayObjectMember => (RuleCode::S100, Diverges(Divergence::ArrayObjectMember)),
            Self::RegexUnsupportedFlag => (RuleCode::S100, TscRejects),
            Self::RegexDuplicateFlag => (RuleCode::S100, TscRejects),
            Self::RegexUnicodeFlagsConflict => (RuleCode::S100, TscRejects),
            Self::ValueTypeArgumentCount => {
                (RuleCode::S100, Diverges(Divergence::ValueTypeArgumentCount))
            }
            Self::ValueTypeOptionsNonLiteral => (
                RuleCode::S100,
                Diverges(Divergence::ValueTypeOptionsNonLiteral),
            ),
            Self::ValueTypeOptionCount => {
                (RuleCode::S100, Diverges(Divergence::ValueTypeOptionCount))
            }
            Self::ValueTypeOptionSpread => {
                (RuleCode::S100, Diverges(Divergence::ValueTypeOptionSpread))
            }
            Self::ValueTypeOptionPropertyForm => (
                RuleCode::S100,
                Diverges(Divergence::ValueTypeOptionPropertyForm),
            ),
            Self::ValueTypeOptionKey => (RuleCode::S100, Diverges(Divergence::ValueTypeOptionKey)),
            Self::ValueTypeAlignmentNonLiteral => (
                RuleCode::S100,
                Diverges(Divergence::ValueTypeAlignmentNonLiteral),
            ),
            Self::ValueTypeAlignmentOutsideSet => (
                RuleCode::S100,
                Diverges(Divergence::ValueTypeAlignmentOutsideSet),
            ),
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
            Self::ArrayElementDomain => (RuleCode::S014, Diverges(Divergence::MethodTypeDomain)),
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

#[path = "rejection_diagnostic.rs"]
mod emission;
pub(crate) use emission::{diagnostic, RejectionClass};
#[path = "rejection_failure.rs"]
mod failure;
pub(crate) use failure::RejectionFailure;
