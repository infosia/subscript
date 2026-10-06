//! The divergence table (`specs/blocks/compiler.md` §79 rule 1).
//!
//! A reject diagnostic at a construct that stock `tsc` accepts is a
//! divergence from TypeScript. [`Divergence`] names one such topic, and
//! [`Divergence::entry`] gives the four facts a diagnostic shows: the
//! TypeScript form, the subscript form for the same intent, the reason,
//! and the record id.
//!
//! One `match` holds the content. No other place holds a fragment.
//!
//! The `collision` id is a `collisions.md` heading id (`C1`..`C15`)
//! where the record has one. Where it has none, the id names the
//! section that decided the rule (`compiler.md §67`, `stdlib.md §10`,
//! or `collisions.md Q29`).

/// One divergence topic: a construct that TypeScript accepts and this
/// language rejects.
///
/// A topic covers every reject corpus entry with the same reason, so one
/// variant can stand behind several entries.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Divergence {
    /// The language has no void[] value.
    PromiseAllVoidValue,
    /// Promise.all derives its result type from its input array; explicit type arguments are outside the admitted form.
    PromiseAllTypeArguments,
    /// Promise.all takes a homogeneous array of async handles. Other iterables, thenables, and ordinary values have no aggregate registration.
    PromiseAllInput,
    /// An aggregate copies result bytes. An async handle or an array of async handles requires the counted store path.
    PromiseAllCountedResult,
    /// An inferred function value has optional parameters only in TypeScript.
    FunctionValueOptionalArguments,
    /// A null check of an element changes no type (§163).
    IndexedReadNullCheck,
    /// The language has no variadic parameters.
    RestParameter,
    /// A string-valued enum member has no integer enum representation.
    StringEnumMemberValue,
    /// A static method accesses a member through this.
    ThisStaticMethodMember,
    /// A source abstract method has no body.
    AbstractMethodBodyMissing,
    /// A call loads a module dynamically.
    DynamicImportCall,
    /// Equality rejects types that TypeScript can compare after numeric erasure.
    ErasedAssignableEquality,
    /// No storage initialization is decided for a declare field.
    DeclaredFieldWithoutValue,
    /// Types require identical arguments; sized numerics erase to number in TypeScript.
    ErasedAssignableTypeMismatch,
    /// Only nullable types carry a null value.
    NonNullableNullEquality,
    /// A const assertion has no decided literal-preservation lowering.
    ConstAssertionExpression,
    /// The only type qualifier is a namespace import; an annotation names a declared or builtin type.
    ImportAnnotation,
    /// String concat takes exactly one string argument.
    StringConcatArgumentCount,
    /// Array concat takes exactly one array argument.
    ArrayConcatArgumentCount,
    /// The coroutine next call drives the frame without an input value.
    GeneratorNextArgumentCount,
    /// The array push surface appends exactly one element.
    ArrayPushArgumentCount,
    /// Types require identical arguments and parameters; the language has no implicit conversion, structural substitution, or variance.
    FunctionParameterIdentity,
    /// The admitted surface excludes this source form.
    ArrayToFixedArray,
    /// The language has no implicit conversion, and an assertion does not check membership.
    EnumToInteger,
    /// The admitted surface excludes this source form.
    LiteralAliasToString,
    /// The lib surface is a subset, and a C function pointer has no properties.
    LiteralAliasMethod,
    /// The lib surface is a subset, and a C function pointer has no properties.
    EnumMethod,
    /// The lib surface is a subset, and a C function pointer has no properties.
    FunctionMethod,
    /// The lib surface is a subset, and a C function pointer has no properties.
    BooleanMethod,
    /// The lib surface is a subset, and a C function pointer has no properties.
    LiteralAliasMember,
    /// The lib surface is a subset, and a C function pointer has no properties.
    EnumMember,
    /// A method value loses its receiver; a bound value captures it, and capturing values cannot escape.
    GeneratorMember,
    /// The lib surface is a subset, and a C function pointer has no properties.
    FunctionMember,
    /// The lib surface is a subset, and a C function pointer has no properties.
    BooleanMember,
    /// No lowering is decided for this operator or expression form.
    MetaPropertyExpression,
    /// No lowering is decided for this operator or expression form.
    ClassExpression,
    /// No lowering is decided for this operator or expression form.
    TaggedTemplateExpression,
    /// No lowering is decided for this operator or expression form.
    CommaExpression,
    /// No lowering is decided for this operator or expression form.
    InstantiationExpression,
    /// No lowering is decided for this operator or expression form.
    SatisfiesExpression,
    /// No lowering is decided for this operator or expression form.
    AngleAssertionExpression,
    /// The language has no implicit conversion, and an assertion does not check membership.
    StringAliasAssertion,
    /// The admitted surface excludes this source form.
    NullableClassAssertion,
    /// The language has no implicit conversion, and an assertion does not check membership.
    IntegerEnumAssertion,
    /// The language has no implicit conversion, and an assertion does not check membership.
    IdentityAssertion,
    /// No lowering is decided for relational comparisons on two boolean operands.
    BooleanRelationalOperand,
    /// The language has no implicit conversion, and an assertion does not check membership.
    BinaryStringOperand,
    /// No lowering is decided for relational comparisons on two string operands.
    StringRelationalOperand,
    /// The language has no implicit conversion, and an assertion does not check membership.
    BinaryEnumOperand,
    /// The language has no implicit conversion, and an assertion does not check membership.
    CompoundStringOperand,
    /// The language has no implicit conversion, and an assertion does not check membership.
    CompoundEnumOperand,
    /// No lowering is decided for this operator or expression form.
    ExponentOperator,
    /// No lowering is decided for this operator or expression form.
    InOperator,
    /// No lowering is decided for this operator or expression form.
    UnaryPlusOperator,
    /// No lowering is decided for this operator or expression form.
    VoidOperator,
    /// No lowering is decided for this operator or expression form.
    TypeofOperator,
    /// An annotation names a declared or builtin type; types are nominal.
    FunctionTypeObjectPattern,
    /// An annotation names a declared or builtin type; types are nominal.
    FunctionTypeArrayPattern,
    /// An annotation names a declared or builtin type; types are nominal.
    FunctionTypeRestParameter,
    /// The admitted surface excludes this source form.
    BigIntAnnotation,
    /// The admitted surface excludes this source form.
    SymbolAnnotation,
    /// An annotation names a declared or builtin type; types are nominal.
    UnknownAnnotation,
    /// An annotation names a declared or builtin type; types are nominal.
    NeverAnnotation,
    /// An annotation names a declared or builtin type; types are nominal.
    TemplateLiteralAnnotation,
    /// An annotation names a declared or builtin type; types are nominal.
    BigIntLiteralAnnotation,
    /// An annotation names a declared or builtin type; types are nominal.
    BooleanLiteralAnnotation,
    /// An annotation names a declared or builtin type; types are nominal.
    NumberLiteralAnnotation,
    /// An annotation names a declared or builtin type; types are nominal.
    StringLiteralAnnotation,
    /// An annotation names a declared or builtin type; types are nominal.
    PredicateAnnotation,
    /// An annotation names a declared or builtin type; types are nominal.
    MappedAnnotation,
    /// An annotation names a declared or builtin type; types are nominal.
    IndexedAnnotation,
    /// An annotation names a declared or builtin type; types are nominal.
    OperatorAnnotation,
    /// An annotation names a declared or builtin type; types are nominal.
    ConditionalAnnotation,
    /// An annotation names a declared or builtin type; types are nominal.
    StructuralAnnotation,
    /// An annotation names a declared or builtin type; types are nominal.
    QueryAnnotation,
    /// An annotation names a declared or builtin type; types are nominal.
    ThisAnnotation,
    /// The admitted surface excludes this source form.
    TupleAnnotation,
    /// No lowering is decided for ECMAScript private members, static blocks, accessor fields, or instance generator methods.
    AutoAccessorDeclaration,
    /// No lowering is decided for ECMAScript private members, static blocks, accessor fields, or instance generator methods.
    StaticBlockDeclaration,
    /// No lowering is decided for ECMAScript private members, static blocks, accessor fields, or instance generator methods.
    PrivateMethodDeclaration,
    /// No lowering is decided for ECMAScript private members, static blocks, accessor fields, or instance generator methods.
    PrivateFieldDeclaration,
    /// No lowering is decided for these statements; for-in reads dynamic properties.
    DebuggerStatement,
    /// No lowering is decided for these statements; for-in reads dynamic properties.
    LabeledStatement,
    /// No lowering is decided for these statements; for-in reads dynamic properties.
    ForInStatement,
    /// No lowering is decided for these statements; for-in reads dynamic properties.
    DoWhileStatement,
    /// The admitted surface excludes this source form.
    SourceNamespaceDeclaration,
    /// An interface is structural, and types are nominal.
    SourceInterfaceDeclaration,
    /// No lowering is decided for a local declaration.
    LocalInterfaceDeclaration,
    /// No lowering is decided for a local declaration.
    LocalAliasDeclaration,
    /// No lowering is decided for a local declaration.
    LocalEnumDeclaration,
    /// No lowering is decided for a local declaration.
    LocalFunctionDeclaration,
    /// No lowering is decided for a local declaration.
    LocalClassDeclaration,
    /// No lowering is decided for type-parameter defaults.
    TypeParameterDefault,
    /// Types require identical arguments and parameters; the language has no implicit conversion, structural substitution, or variance.
    CollectionCallbackTypeMismatchForm,
    /// No lowering is decided for ECMAScript private members, static blocks, accessor fields, or instance generator methods.
    PrivateMemberAssignmentForm,
    /// No lowering is decided for this operator or expression form.
    NonPlaceAssignmentTargetForm,
    /// No lowering is decided for this operator or expression form.
    LogicalOrPowerAssignmentForm,
    /// A lambda captures const locals only; a function expression binds its own this.
    FunctionExpressionForm,
    /// No lowering is decided for this operator or expression form.
    NonNullAssertionExpressionForm,
    /// A parameter default executes outside the method frame that holds the receiver.
    ThisInParameterDefaultArrow,
    /// A ValueType receiver capture copies the receiver.
    ThisInValueTypeArrow,
    /// Classes lower to C layouts and enums to integer constants; neither has a run-time object.
    EnumObjectMember,
    /// No first-class value is decided for a direct call target.
    StaticMethodValueForm,
    /// Classes lower to C layouts and enums to integer constants; neither has a run-time object.
    ConstructorNotNamedClassForm,
    /// A generator supplies one yield type through next, with a zero finished value; no lowering is decided for yield delegation.
    CoroutineReturnOrThrowCallForm,
    /// The yield type comes from the generator body.
    GeneratorYieldTypeNotKnownForm,
    /// The deterministic lib subset excludes this global call.
    LibGlobalCall,
    /// A descriptor literal fills data members by identifier.
    DescriptorLiteralAccessorForm,
    /// A descriptor literal fills data members by identifier.
    DescriptorLiteralQuotedKeyForm,
    /// A descriptor literal fills data members by identifier.
    DescriptorLiteralSpreadForm,
    /// The language has no any array or evolving array type.
    EmptyArrayInferenceForm,
    /// The deterministic lib subset excludes this global name as a value.
    LibGlobalValue,
    /// No first-class value is decided for a direct call target.
    AmbientFunctionValueForm,
    /// No first-class value is decided for a direct call target.
    ForeignFunctionValueForm,
    /// Classes lower to C layouts and enums to integer constants; neither has a run-time object.
    EnumObjectValueForm,
    /// Generic function values require instantiation outside the admitted inference surface.
    GenericFunctionValue,
    /// Interpolation formats scalars, strings, enums, and literal aliases.
    TemplateInterpolationKindForm,
    /// The language uses i64 and u64 and has no BigInt.
    BigIntLiteral,
    /// Enum, integer, and string switches need a default to prove complete return flow.
    LambdaReturnFlowCoverage,
    /// A generator supplies one yield type through next, with a zero finished value; no lowering is decided for yield delegation.
    GeneratorResultValueWriteForm,
    /// A method value loses its receiver; a bound value captures it, and capturing values cannot escape.
    GeneratorResultDoneWriteForm,
    /// A method value loses its receiver; a bound value captures it, and capturing values cannot escape.
    StringMethodValueForm,
    /// A method value loses its receiver; a bound value captures it, and capturing values cannot escape.
    SetMethodValueForm,
    /// A method value loses its receiver; a bound value captures it, and capturing values cannot escape.
    MapMethodValueForm,
    /// A method value loses its receiver; a bound value captures it, and capturing values cannot escape.
    FixedArrayMethodValueForm,
    /// A method value loses its receiver; a bound value captures it, and capturing values cannot escape.
    ArrayMethodValueForm,
    /// Indices are i32; an out-of-range fixed index always traps, and string indexing can give undefined.
    NonIndexableReceiverForm,
    /// Indices are i32; an out-of-range fixed index always traps, and string indexing can give undefined.
    FixedArrayConstantIndexBoundsForm,
    /// Indices are i32; an out-of-range fixed index always traps, and string indexing can give undefined.
    FixedArrayIndexNotIntForm,
    /// Indices are i32; an out-of-range fixed index always traps, and string indexing can give undefined.
    ArrayIndexNotIntForm,
    /// No lowering is decided for ECMAScript private members, static blocks, accessor fields, or instance generator methods.
    PrivateMemberReadForm,
    /// A generator supplies one yield type through next, with a zero finished value; no lowering is decided for yield delegation.
    YieldDelegationForm,
    /// The language has no implicit conversion in a truth test.
    ConditionalNonBooleanConditionForm,
    /// The language has no implicit conversion in a truth test.
    LogicalNonBooleanOperandForm,
    /// The language has no implicit conversion in a truth test.
    LogicalNotNonBooleanForm,
    /// Types require identical arguments and parameters; the language has no implicit conversion, structural substitution, or variance.
    DistinctNominalContainerAssignmentForm,
    /// Classes lower to C layouts and enums to integer constants; neither has a run-time object.
    ConstructorTypeAnnotationForm,
    /// An annotation names a declared or builtin type; types are nominal.
    IntersectionTypeAnnotationForm,
    /// The deterministic lib subset excludes this global type name.
    LibTypeName,
    /// An annotation names a declared or builtin type; types are nominal.
    GeneratorYieldTypeMissingForm,
    /// Array in type position is a builtin that no declaration shadows.
    ArrayTypeArgument,
    /// An annotation names a declared or builtin type; types are nominal.
    QualifiedSourceTypeNameForm,
    /// A function result annotation states the contract that the host binds.
    FunctionReturnAnnotationMissingForm,
    /// A block lambda needs a result annotation.
    BlockLambdaReturnAnnotationMissingForm,
    /// A generic-class default needs an annotated parameter type.
    GenericClassDefaultParameterAnnotationNeeded,
    /// A recursive function value needs an annotated parameter signature.
    FunctionValueParameterAnnotationNeeded,
    /// A generic callback needs an annotated parameter until contextual inference supports it.
    GenericCallbackParameterAnnotationNeeded,
    /// The program is its files; node cannot load an absent module.
    NamedImportModuleMissingForm,
    /// The program is its files; node cannot load an absent module.
    NamespaceImportTargetMissingForm,
    /// A mirror declares C header items, and C has no namespace.
    MirrorModuleDeclarationForm,
    /// No lowering is decided for transparent or generic aliases, or repeated literal members.
    DuplicateLiteralAliasMemberForm,
    /// No lowering is decided for transparent or generic aliases, or repeated literal members.
    SourceAliasNotLiteralUnionForm,
    /// No lowering is decided for transparent or generic aliases, or repeated literal members.
    GenericSourceAliasForm,
    /// No lowering is decided for string-literal enum member names or constructor parameter properties.
    EnumStringMemberNameForm,
    /// No lowering is decided for string-literal enum member names or constructor parameter properties.
    ConstructorParameterPropertyForm,
    /// Initializer inference does not infer a field type from constructor assignments.
    FieldTypeWithoutInitializer,
    /// Field names must be identifiers.
    IdentifierFieldName,
    /// No lowering is decided for ECMAScript private members, static blocks, accessor fields, or instance generator methods.
    GeneratorMethodForm,
    /// Switch dispatch uses constant scalar cases; for await reads async iteration; no lowering is decided for assigning iteration heads.
    SwitchDiscriminantKindForm,
    /// Spread arguments require variadic parameters, which the language does not have.
    ForOfSpreadCall,
    /// Switch dispatch uses constant scalar cases; for await reads async iteration; no lowering is decided for assigning iteration heads.
    ForOfBindingKindForm,
    /// The language has no undefined value for the function-wide var binding.
    ForOfVarBindingForm,
    /// Switch dispatch uses constant scalar cases; for await reads async iteration; no lowering is decided for assigning iteration heads.
    AsyncForOfForm,
    /// The language has no implicit conversion in a truth test.
    StatementNonBooleanConditionForm,
    /// A finished generator result holds the zero value, so a return value has no slot.
    GeneratorReturnValue,
    /// A local without an initializer needs an annotation to define its storage.
    LocalTypeWithoutInitializerForm,
    /// The language has no undefined value for the function-wide var binding.
    LocalVarDeclarationForm,
    /// Enum, integer, and string switches need a default to prove complete return flow.
    ReturnFlowCoverage,
    /// A binding needs an initializer because the language has no undefined value.
    ModuleInitializerMissingForm,
    /// The language has no undefined value for the function-wide var binding.
    ModuleVarDeclarationForm,
    /// The deterministic lib subset excludes this constructor.
    LibConstructorName,
    /// No first-class value is decided for a direct call target.
    GeneratorFunctionValueForm,
    /// A method value loses its receiver; a bound value captures it, and capturing values cannot escape.
    GenericSynchronousMethodValueForm,
    /// A method value loses its receiver; a bound value captures it, and capturing values cannot escape.
    SynchronousMethodValueForm,
    /// A Map copy traverses a live source Map; a nullable source does not supply that map.
    MapCopyNullableSource,

    /// A nominal class has only its declared members; it does not inherit the JavaScript Object API.
    ClassInheritedObjectMember,
    /// A class names its nominal type and static declarations; the language has no runtime constructor object.
    ClassRuntimeObject,

    /// Unary negation takes a sized numeric operand; it does not coerce a string to a number.
    UnaryNumericCoercion,
    /// Bitwise operators take sized integer operands and preserve their integer width.
    BitwiseIntegerOperand,
    /// A class has a fixed C layout; delete cannot remove a property. Context.free releases an entire instance.
    DeleteProperty,
    /// The optional-chain surface tests a nullable receiver for a field read or a method call; it does not test a callable.
    OptionalMethodCall,
    /// The optional-chain surface tests a nullable receiver for a field read or a method call; it does not test a callable.
    OptionalFunctionCall,
    /// The undefined token appears only in a presence comparison with an absence-capable descriptor member.
    UndefinedEqualityPair,
    /// The undefined token appears only in a presence comparison with an absence-capable descriptor member.
    UndefinedEqualityNonMember,
    /// A bare yield carries no value, so only a void generator can use it.
    BareYieldNonVoid,
    /// An async method is a direct call target and has no first-class function value.
    AsyncMethodValue,
    /// A generic async method is a direct call target and has no first-class function value.
    GenericAsyncMethodValue,
    /// A fixed array supplies length, numeric elements, and the admitted callback methods; it has no object reflection members.
    FixedArrayObjectMember,
    /// A Map supplies the declared container API; it has no Object reflection members.
    MapObjectMember,
    /// A Set supplies the declared container API; it has no Object reflection members.
    SetObjectMember,
    /// A coroutine step result contains done and value only; it has no Object methods.
    GeneratorResultObjectMember,
    /// A sized numeric supplies the admitted Number formatting methods; it has no Object reflection members.
    NumericObjectMember,
    /// The boundary object type has no field shape; member access requires a checked class narrowing.
    BoundaryObjectMember,
    /// The Worker static surface supplies spawn only; it has no Object methods.
    WorkerStaticObjectMethod,
    /// A Worker supplies post, poll, close, and join only; it has no Object methods.
    WorkerObjectMethod,
    /// An Inbox supplies wait and poll only; it has no Object methods.
    InboxObjectMethod,
    /// An Outbox supplies post only; it has no Object methods.
    OutboxObjectMethod,
    /// A fixed array supplies length, numeric elements, and the admitted callback methods; it has no Object methods.
    FixedArrayObjectMethod,
    /// A sized numeric supplies the admitted Number formatting methods; it has no Object methods.
    NumericObjectMethod,
    /// A string supplies the admitted UTF-8 string API; it has no Object reflection members.
    StringObjectMember,
    /// A Map supplies the admitted container API; it has no Object methods.
    MapObjectMethod,
    /// A Set supplies the admitted container API; it has no Object methods.
    SetObjectMethod,
    /// An array supplies the admitted container API; it has no Object reflection members.
    ArrayObjectMember,
    /// A RegExp pattern is fixed at construction; the admitted surface has no compile method.
    RegexCompile,
    /// A fractional literal requires a floating context; an integer context cannot represent its fraction.
    FractionalIntegerLiteral,
    /// A fixed-array annotation sets its C array length; the constructing literal must supply exactly that many elements.
    FixedArrayLiteralLength,
    /// A byte operation reads the exact declared aggregate type; a structurally compatible aggregate has a different storage layout.
    ByteArgumentIdentity,
    /// A generic constructor requires explicit type arguments; constructor argument inference is outside the admitted generic-call surface.
    GenericConstructorTypeArguments,
    /// Worker.spawn takes one directly named entry function; a spread argument does not supply that direct source form.
    WorkerSpawnSpread,
    /// A worker entry must directly name a module-level function; a local function value has no entry identity.
    WorkerEntryLocalValue,
    /// A worker entry must name a non-generic module-level function with the exact synchronous entry shape.
    WorkerEntryGeneric,
    /// A worker entry must have exactly two endpoint parameters, return void, and have no default parameter.
    WorkerEntrySignature,
    /// A worker entry uses the builtin Inbox and Outbox identities; same-shaped source classes are different endpoint types.
    WorkerEntryStructuralEndpoints,
    /// Explicit Worker.spawn arguments must name the entry message classes; structurally equal classes have different nominal identities.
    WorkerExplicitMessageIdentity,
    /// An await consumes an async handle or a direct admitted async call; a synchronous value carries no completion.
    AwaitNonHandle,
    /// An awaited named call must resolve to a declared async function; an ambient synchronous call carries no async completion.
    AwaitUndeclaredAsyncFunction,
    /// An await requires an async completion; a synchronous function call supplies only its immediate result.
    AwaitSynchronousFunction,
    /// The admitted awaited method form directly names the instance method with an identifier.
    AwaitComputedMethod,
    /// An awaited instance method belongs to an admitted reference class; a primitive method is synchronous.
    AwaitNonClassMethod,
    /// An await requires an async completion; a synchronous method call supplies only its immediate result.
    AwaitSynchronousMethod,
    /// An indirect call returns a synchronous value with no async completion.
    AwaitIndirectCall,
    /// The unshift surface inserts exactly one element; it has no zero-element overload.
    ArrayUnshiftEmpty,
    /// An array callback receives values and an optional index; the callback API has no thisArg parameter.
    ArrayCallbackThisArgument,
    /// Map.forEach takes one callback; its API has no thisArg parameter.
    MapCallbackThisArgument,
    /// Set.forEach takes one callback; its API has no thisArg parameter.
    SetCallbackThisArgument,
    /// Map.groupBy takes a dynamic array; a source class that inherits Array has a different nominal identity.
    MapGroupByArraySource,
    /// Map.groupBy needs a storable key value; a void callback supplies no key.
    MapGroupByVoidKey,

    /// A bare null initializer supplies no inferred type, so the declaration needs an explicit nullable annotation.
    NullInitializerInference,
    /// A using binding must name a reference resource class with a disposal hook, optionally nullable.
    UsingBindingResourceType,
    /// A mirror declares the ambient C surface directly and cannot declare an export list.
    MirrorExportList,
    /// One declaration owns each top-level name in a module; a second declaration cannot add an overload or a merged declaration.
    TopLevelNameClash,
    /// The module surface uses export declarations, named export lists, named imports, and namespace imports only.
    UnsupportedModuleDeclaration,
    /// A default import stays outside the named module surface, even when module discovery marks the source absent.
    PoisonedDefaultImport,
    /// A default import stays outside the named module surface.
    DefaultImport,
    /// Only the disposal hook has a computed method name; every other computed method name is rejected.
    ComputedMethodName,
    /// A source function needs a body; an ambient function belongs in a host mirror.
    FunctionBodyMissing,
    /// A static field needs an initializer because its module storage must start with a value.
    StaticFieldInitializerMissing,
    /// A field assignment counts only before every constructor statement that contains a return.
    FieldAssignmentAfterUnreachableReturn,
    /// Every ordinary instance field needs an initializer or a top-level constructor assignment, even when the constructor always throws.
    FieldAssignmentMissingNoNormalExit,
    /// A constructor reads a field only after an initializer or an earlier top-level statement gives it a value.
    ConstructorFieldReadWithAssignmentFact,
    /// Disposal hooks run synchronously; an await using loop binding is outside this disposal surface.
    ForOfAwaitUsing,
    /// A switch over a string-literal alias requires each case label to spell a member literal.
    AliasCaseNonLiteral,
    /// One field, method, or accessor pair owns each name in its class member namespace.
    ClassMemberNameClash,
    /// A descriptor class contains data only and declares no method or accessor.
    DescriptorMethod,
    /// A mirror class describes C instance storage and declares no static method or accessor.
    MirrorStaticMethod,
    /// A disposal hook must be an instance method.
    DisposeStatic,
    /// A mirror class reads C fields and does not declare a script accessor.
    MirrorAccessor,
    /// A read accessor must declare an explicit return type.
    ReadAccessorReturnMissing,
    /// A write accessor must declare one named parameter with an explicit type.
    WriteAccessorPattern,
    /// A write accessor must declare an explicit parameter type.
    WriteAccessorTypeMissing,
    /// A generic method template needs a body at collection; a separate overload signature has no template body.
    GenericMethodBodyMissing,
    /// A disposal hook must run synchronously.
    DisposeAsync,
    /// A disposal hook takes no parameter and returns void.
    DisposeSignature,
    /// A descriptor class declares its own data and does not inherit.
    DescriptorInheritance,
    /// A reference class has its own nominal identity and C layout; the language has no class inheritance.
    ReferenceClassInheritance,
    /// A descriptor class declares instance data only and has no static field.
    DescriptorStaticField,
    /// A mirror class describes C instance storage and has no static field.
    MirrorStaticField,
    /// A static field cannot represent undefined; optional fields belong only to the descriptor surface.
    StaticFieldOptional,
    /// Worker, Inbox, and Outbox values belong to one Context and cannot enter static field storage.
    ContextAffineStaticField,
    /// A descriptor member with a default initializer must use the optional question-mark spelling.
    DescriptorInitializerWithoutOptional,
    /// A required descriptor member must use the definite-assignment spelling so a constructing literal supplies its value.
    DescriptorRequiredWithoutDefinite,
    /// An ordinary instance field cannot represent undefined; optional fields belong only to the descriptor surface.
    InstanceFieldOptional,
    /// A wire alias occupies a direct boundary field or an array-pair element; a nested boundary type has no declared wire position.
    WireAliasNestedField,
    /// Worker, Inbox, and Outbox values belong to one Context and cannot enter instance field storage.
    ContextAffineInstanceField,
    /// A value class contains only sized numerics, booleans, value classes, fixed arrays, enums, and admitted literal aliases.
    ValueFieldOutsideWhitelist,
    /// A descriptor class receives its fields from a constructing literal and declares no constructor.
    DescriptorConstructor,
    /// A wire alias is a direct mirror constructor parameter or an array-pair element; other nested positions have no wire representation.
    WireAliasNestedConstructorParameter,
    /// A class declares at most one index signature for its accessor pair.
    ClassIndexSignatureCount,
    /// Only an ordinary reference class can declare an index signature.
    ClassIndexSignatureNonReference,
    /// A class index signature describes instance access through its get and set methods.
    ClassIndexSignatureStatic,
    /// A class index signature requires an i32 or u32 index.
    ClassIndexSignatureIndexType,
    /// A write accessor needs a read accessor because property writes use the shared accessor type.
    WriteAccessorWithoutRead,
    /// A read accessor and its write accessor must share exactly one type.
    AccessorTypeMismatch,
    /// A mutable class index signature needs a synchronous set method with exactly matching index and element types.
    ClassIndexSetSignature,
    /// A using binding needs a function block scope for deterministic disposal; module bindings have no such exit.
    ModuleUsing,
    /// The Descriptor decorator accepts no options.
    DescriptorOptions,
    /// Only the ambient ValueType and Descriptor decorators define a class representation.
    UnsupportedClassDecorator,
    /// A descriptor is a reference class and cannot also declare value-class representation.
    DescriptorValueType,
    /// An enum member occupies i32 storage, so an implicit next value must fit the i32 range.
    EnumImplicitValueOverflow,
    /// A CEnum wire mapping must contain at least one member.
    WireEnumEmpty,
    /// A CEnum mapping consists of named properties whose types are integer literals, rather than methods or index signatures.
    WireEnumMemberForm,
    /// A CEnum member key must name a string-literal union member, so numeric property keys are excluded.
    WireEnumMemberKey,
    /// A mirror binds constants with non-negative integer-literal values; it does not bind a host data symbol.
    MirrorVariableForm,
    /// A wire alias is a direct foreign parameter or an array-pair element; another nested position has no declared wire representation.
    WireAliasNestedForeignParameter,
    /// A wire alias has a direct foreign return representation only; an array return has no declared wire representation.
    WireAliasNestedForeignReturn,
    /// A callback occupies a mirrored boundary-struct field; a direct foreign callback parameter has no declared provenance position.
    ForeignDirectCallback,
    /// A foreign string-view, descriptor, or callback return has no return provenance in the boundary vocabulary.
    ForeignReturnProvenance,
    /// An async function uses a Promise<T> return view; an async generator needs an async iterator return view.
    AsyncGeneratorFunction,
    /// An async function must declare its suspendable return view with an explicit Promise<T> annotation.
    AsyncReturnAnnotationMissing,
    /// An optional parameter implies undefined, which has no language value.
    OptionalParameter,
    /// An Error message must be a string; a nominal literal alias needs explicit formatting.
    ErrorMessageType,
    /// An Error constructor accepts an optional string message only; options and spread arguments are outside its surface.
    ErrorConstructorArguments,
    /// An Error-family object must use the new constructor spelling.
    ErrorCallWithoutNew,
    /// The ValueType decorator requires exactly one object-literal argument without a spread.
    ValueTypeArgumentCount,
    /// The ValueType decorator requires an object-literal options argument.
    ValueTypeOptionsNonLiteral,
    /// A ValueType options literal must contain exactly the align property.
    ValueTypeOptionCount,
    /// A ValueType options literal declares align directly and does not spread properties.
    ValueTypeOptionSpread,
    /// A ValueType options literal declares align as a key-value property.
    ValueTypeOptionPropertyForm,
    /// A ValueType options literal accepts only the align key.
    ValueTypeOptionKey,
    /// A ValueType alignment must be a numeric integer literal.
    ValueTypeAlignmentNonLiteral,
    /// A ValueType alignment must be one of 2, 4, 8, or 16.
    ValueTypeAlignmentOutsideSet,
    /// A program with multiple source files must name exactly one non-ambient entry module in its build input.
    InvalidProgramEntry,
    /// An async function must spell its suspendable return view as Promise<T>.
    AsyncReturnNonReference,
    /// An async function must spell its suspendable return view with the unqualified Promise<T> name.
    AsyncReturnQualifiedName,
    /// An async function must spell its suspendable return view as Promise<T>, rather than an alias.
    AsyncReturnAlias,
    /// The suspendable return view must state exactly one fulfilled-value type.
    AsyncReturnMissingArgument,
    /// The suspendable return view must state exactly one fulfilled-value type.
    AsyncReturnArgumentCount,
    /// A FixedArray must fit the signed aggregate byte limit, which excludes lengths above the element-count range.
    FixedArrayLengthRange,
    /// A C array layout needs a non-negative integer literal length at compile time.
    FixedArrayLengthLiteral,
    /// A generic constraint uses nominal class identity, rather than TypeScript structural compatibility.
    GenericConstraintIdentity,
    /// A namespace qualifier exposes only the declared named exports; a default export stays outside the module surface.
    NamespaceUnexportedMember,
    /// The corpus runner calls the entry module host function main; a library export cannot supply that entry.
    RunnerMainMissing,

    /// Final alignment contributes bytes to the object layout and must stay within the signed displacement limit.
    ClassFinalAlignmentLimit,
    /// Argument copies occupy stack storage and must stay within the accumulated frame limit.
    AggregateArgumentFrameLimit,
    /// A namespace import is a static qualifier and has no runtime value.
    NamespaceAsValue,
    /// A closure can run before the initializer of a binding in another case.
    SwitchCaseClosureRead,
    /// A block owns each declared name throughout its scope; an earlier read must not resolve to an outer declaration.
    BlockNameReadBeforeDeclaration,
    /// A block owns each declared name throughout its scope; an earlier write must not target an outer declaration.
    BlockNameWriteBeforeDeclaration,
    /// A worker handle belongs to one Context and cannot enter a closure environment.
    ContextAffineCapture,
    /// A closure copies const local values; mutable captures need shared storage that the language does not provide.
    MutableLocalCapture,
    /// A worker handle belongs to one Context and cannot enter array storage.
    ContextAffineArrayElement,
    /// A worker handle belongs to one Context and cannot enter container storage.
    ContextAffineContainerArgument,
    /// Worker messaging copies a plain reference class; scalar and decorated message types have no declared transfer shape.
    WorkerMessagePlainClass,
    /// A string-literal alias has no C wire representation for a mirror signature.
    BoundaryLiteralAlias,
    /// Only reference classes, opaque handles, function types, and boundary pointers have a nullable representation.
    NullableNonReference,
    /// A value class stores its fields inline and has no nullable pointer representation.
    NullableValueClassAssignment,
    /// Foreign functions need a header identity for C emission.
    MirrorHeaderMissing,
    /// Parameter provenance must name a parameter declared in the same mirror.
    MirrorParameterTargetMissing,
    /// Callback provenance must name a callback typedef declared in the same mirror.
    MirrorCallbackTargetMissing,
    /// A callback lifetime record must name a boundary class that carries a callback field.
    MirrorLifetimeTargetMissing,
    /// An absorbed array parameter needs its C aggregate or scalar-pair provenance for C emission.
    MirrorArrayProvenanceMissing,
    /// An absorbed string parameter needs its C string-view aggregate identity for C emission.
    MirrorStringProvenanceMissing,
    /// The declared parameter type must agree with its recorded C parameter shape.
    MirrorParameterProvenanceMismatch,
    /// A callback needs a named C typedef for its trampoline cast.
    MirrorAnonymousCallback,
    /// A callback needs its recorded C typedef identity for its trampoline cast.
    MirrorCallbackProvenanceMissing,
    /// A provenance record must use a declared record kind.
    ProvenanceUnknownKind,
    /// A provenance record needs a record kind.
    ProvenanceMissingKind,
    /// Whitespace must separate provenance fields.
    ProvenanceFieldSeparator,
    /// Each provenance field must use the key required by its record kind.
    ProvenanceUnexpectedKey,
    /// A provenance string must use quotes.
    ProvenanceUnquotedString,
    /// A provenance string must have a closing quote.
    ProvenanceUnterminatedString,
    /// A provenance escape must contain its escaped character.
    ProvenanceUnterminatedEscape,
    /// A provenance string must use a declared escape form.
    ProvenanceUnsupportedEscape,
    /// A provenance string must escape control characters.
    ProvenanceControlCharacter,
    /// A Unicode escape must contain four hexadecimal digits.
    ProvenanceInvalidUnicodeDigits,
    /// A provenance boolean must use true or false.
    ProvenanceInvalidBoolean,
    /// A provenance record must end after its declared fields.
    ProvenanceTrailingData,
    /// A Unicode escape must contain four hexadecimal digits.
    ProvenanceShortUnicodeEscape,
    /// A provenance string must contain Unicode scalar values.
    ProvenanceInvalidUnicodeScalar,
    /// A header identity must be a nonempty basename without control characters.
    ProvenanceHeaderBasename,
    /// A mirror has one header identity.
    ProvenanceDuplicateHeader,
    /// A descriptor record needs nonempty function, parameter, aggregate, and element identities.
    ProvenanceEmptyDescriptor,
    /// One parameter has one recorded C shape.
    ProvenanceDuplicateDescriptor,
    /// A string-view record needs nonempty function, parameter, and aggregate identities.
    ProvenanceEmptyStringView,
    /// One parameter has one recorded C shape.
    ProvenanceDuplicateStringView,
    /// A scalar-pair record needs nonempty function, parameter, and element identities.
    ProvenanceEmptyScalarPair,
    /// One parameter has one recorded C shape.
    ProvenanceDuplicateScalarPair,
    /// A callback record needs a nonempty C typedef identity.
    ProvenanceEmptyCallback,
    /// One callback typedef has one provenance record.
    ProvenanceDuplicateCallback,
    /// A callback lifetime record needs a nonempty aggregate identity.
    ProvenanceEmptyCallbackLifetime,
    /// One callback aggregate has one lifetime record.
    ProvenanceDuplicateCallbackLifetime,
    /// An external-type record needs a nonempty C type identity.
    ProvenanceEmptyExternalType,
    /// One external type has one provenance record.
    ProvenanceDuplicateExternalType,
    /// A CEnum record needs nonempty typedef and alias identities.
    ProvenanceEmptyCEnum,
    /// One CEnum typedef has one provenance record.
    ProvenanceDuplicateCEnum,

    /// Iteration requires a declared container or string type; literal unions have no traversal representation.
    IterationSubjectDomain,
    /// FixedArray supports the callback family; other compiler-owned array methods require a dynamic array receiver.
    FixedArrayMethods,

    /// Compiler-owned namespaces and methods lower to direct operations; the language has no value or writable storage for them.
    CompilerOwnedValue,
    /// Compiler namespaces expose only declared intrinsics; JavaScript prototype members and inherited Object methods have no namespace representation.
    NamespaceObjectMember,
    /// Unicode normalization needs tables that the runtime does not provide.
    UnicodeNormalization,
    /// TypeScript makes the match index optional; the language requires a definite i32 index and has no optional numeric field.
    MatchOptionalIndex,
    /// A runtime flattening depth cannot determine one static result element type.
    ArrayFlattenDepth,
    /// Each method has a fixed receiver, element, result, and accumulator domain; TypeScript generic method domains include more kinds.
    MethodTypeDomain,
    /// Array join uses the interpolation rules; nested arrays and other non-interpolatable elements have no implicit string form.
    ArrayJoinDomain,
    /// Array spread creates a dynamic array; its runtime length cannot construct a FixedArray with a static length.
    FixedArraySpread,
    /// The intrinsic requires one explicit type argument for its storage or element type; inferred and mapper overloads do not supply that shape.
    ExplicitIntrinsicTypeArguments,
    /// Source construction accepts arrays, FixedArray, Set, and string; null and JavaScript array-like objects are outside this domain.
    SourceConstructionDomain,
    /// A container callback declares its element parameters and an optional index; omitted element parameters do not match the runtime callback ABI.
    CallbackParameterShape,
    /// JSON intrinsics take one argument; replacer, spacing, and reviver overloads are outside the declared interface.
    JsonCallArguments,
    /// JSON helpers require a supported static data shape; RegExp and container parse targets have no helper representation.
    JsonTypeDomain,
    /// String search requires a compiled RegExp; implicit conversion from a string pattern is outside the regular-expression interface.
    StringSearchPattern,
    /// A mirror function uses named C ABI parameters; parameter destructuring requires a script body that a mirror does not provide.
    MirrorParameterPattern,
    /// Locale-sensitive number formatting needs host locale data; the runtime provides only explicit locale-independent formats.
    LocaleNumberFormatting,
    /// User iteration protocols require Symbol.iterator, which the runtime does not provide.
    UserIterationProtocol,
    /// Set algebra requires a native Set argument; a Set subclass has no runtime container representation.
    SetAlgebraDomain,
    /// A surrogate escape without an adjacent paired escape.
    LoneSurrogateEscape,
    /// An inferred `void` binding, a void map callback, or a value return from a void function.
    VoidValue,
    /// A reference-element search miss uses `null` instead of `undefined`.
    ReferenceSearchMiss,
    /// A finished generator value with an invalid zero traps at the read (compiler.md §145).
    GeneratorDoneValue,
    /// `any` in a declaration.
    AnyType,
    /// `eval`, `new Function`, and a write through `.prototype`.
    DynamicObjectModel,
    /// A class instance used where another same-shaped class is wanted.
    NominalClassIdentity,
    /// An object literal used as a plain class instance.
    ObjectLiteralConstruction,
    /// `extends` on a value class, and an alignment below the natural one.
    ValueClassLayout,
    /// Bare `number` in a declaration.
    BareNumber,
    /// Operands and arguments of two different numeric widths.
    SizedOperandWidths,
    /// Arithmetic in the storage-only `f16` type.
    StorageOnlyHalfFloat,
    /// An integer literal outside the range of its contextual type.
    IntegerLiteralRange,
    /// A `CEnum` wire value that is fractional, repeated, or too wide.
    WireEnumValues,
    /// A capturing lambda that escapes, and the container callback parameter.
    EscapingCapture,
    /// A form outside the decided exception surface: a non-Error `throw`,
    /// `finally`, and a catch binding read or annotation outside the two
    /// legal forms.
    Exceptions,
    /// `instanceof` with a class outside the Error family.
    InstanceofNonError,
    /// A general union type, and the `undefined` token.
    GeneralUnionAndUndefined,
    /// A nullish test on a non-nullable value.
    NullishNonNullable,
    /// An optional-chain test on a non-nullable value.
    OptionalChainNonNullable,
    /// A nullish assignment.
    NullishAssignment,
    /// A non-place nullish receiver in an initializer.
    NonPlaceNullishInitializer,
    /// An optional chain that will bind `undefined` in TypeScript.
    OptionalChainUnbound,
    /// A computed optional-chain step.
    OptionalChainIndex,
    /// An inline literal union, and assignment across two aliases.
    LiteralUnionAlias,
    /// An optional descriptor member, its default, and its presence read.
    OptionalDescriptorMember,
    /// The boundary-opaque `object` type in a general declaration.
    BoundaryOnlyObject,
    /// `Promise` construction, statics, and combinators.
    PromiseObject,
    /// `await` in a synchronous function or at the top level.
    AwaitOutsideAsync,
    /// An async static method, generator, value-class method, or generic arrow.
    AsyncFunctionShape,
    /// An async arrow captures a local binding.
    AsyncArrowCapture,
    /// An async return carries a fulfilled value, without handle adoption.
    AsyncReturnHandle,
    /// An async call whose handle no holder awaits.
    DroppedAsyncHandle,
    /// A forbidden `this` form in a field initializer.
    ThisInFieldInitializer,
    /// A class index signature without its accessors, and a compound write.
    ClassIndexSignature,
    /// A rejected disposal hook or a `using` declaration that is `await`ed or inside a lambda.
    UsingDeclaration,
    /// A value-position write, a value-class write accessor, or a mirror accessor.
    NamedAccessor,
    /// A container view held as a value instead of iterated.
    IteratorTemporary,
    /// A name that the two languages resolve to different declarations.
    DeclarationScope,
    /// A wildcard, namespace, default export, `default` export name, or type-only export.
    NamedModuleSurface,
    /// An entry-module export without a supported host signature.
    HostApiSurface,
    /// A module or static initializer that reads a later binding.
    ModuleInitializerOrder,
    /// A recursive generic request whose type arguments grow without bound.
    GrowingInstanceChain,
    /// A static member on a generic class, and `this` in a static method.
    StaticMemberSurface,
    /// `Math` as a value, and the variadic `Math.max`.
    MathSubset,
    /// Local-time, mutable, and current-clock `Date` forms.
    DateSubset,
    /// Locale-sensitive collation and case mapping.
    LocaleSensitiveString,
    /// `sort`, `find`, and `reduce` in their defaulted lib forms.
    ArrayMethodDefaults,
    /// An array method called with a variadic tail.
    VariadicArguments,
    /// A `Map` or `Set` key of a kind with no hash.
    MapKeyKind,
    /// `get` on a scalar-valued `Map`, which has no miss value.
    MapScalarGet,
    /// `get` on a `Map` whose value type has no nullable form (compiler.md §123).
    MapNonNullableGet,
    /// A shared location read after a call, suspension, or alias store.
    SharedLocationNarrowing,
    /// A pair-valued construction or view, which needs a tuple type.
    NoTupleType,
    /// A coercing numeric call, and an omitted radix or digit count.
    NumberCoercionAndArguments,
    /// A `JSON` input or parse target with no static field shape.
    JsonSubset,
    /// An aggregate or a stack frame past its byte limit.
    AggregateLayoutLimit,
    /// `exec`, `matchAll`, `lastIndex`, `groups`, and sticky matching.
    RegExpSubset,
    /// `replaceAll` with a literal that has no `g` flag.
    ReplaceAllGlobalFlag,
    /// A capturing or async `Worker.spawn` entry.
    WorkerEntryShape,
    /// A worker message or handle that leaves its Context.
    WorkerContextAffinity,
    /// A `switch` over a literal-union alias that is partial or repeated.
    SwitchOverAlias,
    /// `unreachable()` in a value position.
    UnreachableInValuePosition,
    /// `new` on a literal-constructible descriptor class.
    DescriptorConstruction,
    /// `Context.bytesOf` on a target with no C-identical layout.
    ByteAccessTarget,
    /// A plain literal-union alias in a host-callable entry signature.
    EntryParameterType,
    /// A chain header copied out of its enclosing extension.
    EmbeddedHeaderCopy,
    /// A generic function call with conflicting inference candidates.
    GenericInferenceCandidates,
    /// A generic function call with no inference candidate for a parameter.
    GenericInferenceMissing,
    /// A generic method call that supplies no type arguments.
    GenericMethodTypeArguments,
    /// A bodiless generic method in a `declare class` from a `.ts` source.
    BodilessDeclareGenericMethod,
    /// A generic method declared on a generic class.
    GenericMethodOnGenericClass,
    /// A `Generator<T>` consumed by a spread or by a Set construction.
    GeneratorSingleUse,
    /// A bare `Map` used as a `for…of` subject.
    BareMapSubject,
    /// A bare `Map` collected into an array, by spread or `Array.from`.
    BareMapToArray,
    /// The `Array.from` mapper overload.
    ArrayFromMapper,
    /// `Array.isArray`.
    ArrayIsArray,
    /// `Array.of` at variable arity.
    ArrayOfArity,
    /// `new Array<T>(length)`.
    ArrayHoleConstruction,
    /// A default value inside a binding pattern.
    PatternDefaultValue,
    /// A rest element in an array binding pattern.
    ArrayRestPattern,
    /// A rest element in a field binding pattern.
    ObjectRestPattern,
    /// A binding pattern inside a binding pattern.
    NestedPattern,
    /// A computed or numeric field name in a binding pattern.
    PatternFieldName,
    /// A binding pattern over a source that is neither an array nor a class.
    PatternSourceShape,
    /// A destructuring assignment to names that already exist.
    AssignmentPattern,
    /// A binding pattern in a declaration outside a function body.
    ModuleLevelPattern,
    /// A `!` field that nothing assigns at the constructor's top level.
    DefiniteAssignmentAssertion,
    /// A field assigned only inside a nested statement of the constructor.
    NestedFieldAssignmentEveryNormalExit,
    /// `new` on a `declare class` that a program file declares.
    AmbientClassConstruction,
    /// `this` that escapes a constructor before every field holds a value.
    ThisBeforeFieldValues,
}

/// The four facts that a divergence diagnostic shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DivergenceEntry {
    /// A TypeScript fragment that stock `tsc` accepts.
    pub ts: &'static str,
    /// The fragment this language accepts for the same intent, or the
    /// sentence `no equivalent; <what to do instead>`.
    pub subscript: &'static str,
    /// One sentence, 25 words or fewer, that gives the reason.
    pub why: &'static str,
    /// The record id: a `collisions.md` heading id (`C1`..`C15`), or a
    /// section id where the record has no heading.
    pub collision: &'static str,
}

impl Divergence {
    /// Every divergence topic, each one time.
    pub const ALL: &'static [Divergence] = &[
        Divergence::FunctionValueOptionalArguments,
        Divergence::IndexedReadNullCheck,
        Divergence::RestParameter,
        Divergence::ErasedAssignableEquality,
        Divergence::DeclaredFieldWithoutValue,
        Divergence::ErasedAssignableTypeMismatch,
        Divergence::NonNullableNullEquality,
        Divergence::DynamicImportCall,
        Divergence::AbstractMethodBodyMissing,
        Divergence::ThisStaticMethodMember,
        Divergence::StringEnumMemberValue,
        Divergence::ImportAnnotation,
        Divergence::ConstAssertionExpression,
        Divergence::StringConcatArgumentCount,
        Divergence::ArrayConcatArgumentCount,
        Divergence::GeneratorNextArgumentCount,
        Divergence::ArrayPushArgumentCount,
        Divergence::FunctionParameterIdentity,
        Divergence::ArrayToFixedArray,
        Divergence::EnumToInteger,
        Divergence::LiteralAliasToString,
        Divergence::LiteralAliasMethod,
        Divergence::EnumMethod,
        Divergence::FunctionMethod,
        Divergence::BooleanMethod,
        Divergence::LiteralAliasMember,
        Divergence::EnumMember,
        Divergence::GeneratorMember,
        Divergence::FunctionMember,
        Divergence::BooleanMember,
        Divergence::MetaPropertyExpression,
        Divergence::ClassExpression,
        Divergence::TaggedTemplateExpression,
        Divergence::CommaExpression,
        Divergence::InstantiationExpression,
        Divergence::SatisfiesExpression,
        Divergence::AngleAssertionExpression,
        Divergence::StringAliasAssertion,
        Divergence::NullableClassAssertion,
        Divergence::IntegerEnumAssertion,
        Divergence::IdentityAssertion,
        Divergence::BooleanRelationalOperand,
        Divergence::BinaryStringOperand,
        Divergence::StringRelationalOperand,
        Divergence::BinaryEnumOperand,
        Divergence::CompoundStringOperand,
        Divergence::CompoundEnumOperand,
        Divergence::ExponentOperator,
        Divergence::InOperator,
        Divergence::UnaryPlusOperator,
        Divergence::VoidOperator,
        Divergence::TypeofOperator,
        Divergence::FunctionTypeObjectPattern,
        Divergence::FunctionTypeArrayPattern,
        Divergence::FunctionTypeRestParameter,
        Divergence::BigIntAnnotation,
        Divergence::SymbolAnnotation,
        Divergence::UnknownAnnotation,
        Divergence::NeverAnnotation,
        Divergence::TemplateLiteralAnnotation,
        Divergence::BigIntLiteralAnnotation,
        Divergence::BooleanLiteralAnnotation,
        Divergence::NumberLiteralAnnotation,
        Divergence::StringLiteralAnnotation,
        Divergence::PredicateAnnotation,
        Divergence::MappedAnnotation,
        Divergence::IndexedAnnotation,
        Divergence::OperatorAnnotation,
        Divergence::ConditionalAnnotation,
        Divergence::StructuralAnnotation,
        Divergence::QueryAnnotation,
        Divergence::ThisAnnotation,
        Divergence::TupleAnnotation,
        Divergence::AutoAccessorDeclaration,
        Divergence::StaticBlockDeclaration,
        Divergence::PrivateMethodDeclaration,
        Divergence::PrivateFieldDeclaration,
        Divergence::DebuggerStatement,
        Divergence::LabeledStatement,
        Divergence::ForInStatement,
        Divergence::DoWhileStatement,
        Divergence::SourceNamespaceDeclaration,
        Divergence::SourceInterfaceDeclaration,
        Divergence::LocalInterfaceDeclaration,
        Divergence::LocalAliasDeclaration,
        Divergence::LocalEnumDeclaration,
        Divergence::LocalFunctionDeclaration,
        Divergence::LocalClassDeclaration,
        Divergence::TypeParameterDefault,
        Divergence::CollectionCallbackTypeMismatchForm,
        Divergence::PrivateMemberAssignmentForm,
        Divergence::NonPlaceAssignmentTargetForm,
        Divergence::LogicalOrPowerAssignmentForm,
        Divergence::FunctionExpressionForm,
        Divergence::NonNullAssertionExpressionForm,
        Divergence::ThisInParameterDefaultArrow,
        Divergence::ThisInValueTypeArrow,
        Divergence::EnumObjectMember,
        Divergence::StaticMethodValueForm,
        Divergence::ConstructorNotNamedClassForm,
        Divergence::CoroutineReturnOrThrowCallForm,
        Divergence::GeneratorYieldTypeNotKnownForm,
        Divergence::LibGlobalCall,
        Divergence::DescriptorLiteralAccessorForm,
        Divergence::DescriptorLiteralQuotedKeyForm,
        Divergence::DescriptorLiteralSpreadForm,
        Divergence::EmptyArrayInferenceForm,
        Divergence::LibGlobalValue,
        Divergence::AmbientFunctionValueForm,
        Divergence::ForeignFunctionValueForm,
        Divergence::EnumObjectValueForm,
        Divergence::GenericFunctionValue,
        Divergence::TemplateInterpolationKindForm,
        Divergence::BigIntLiteral,
        Divergence::LambdaReturnFlowCoverage,
        Divergence::GeneratorResultValueWriteForm,
        Divergence::GeneratorResultDoneWriteForm,
        Divergence::StringMethodValueForm,
        Divergence::SetMethodValueForm,
        Divergence::MapMethodValueForm,
        Divergence::FixedArrayMethodValueForm,
        Divergence::ArrayMethodValueForm,
        Divergence::NonIndexableReceiverForm,
        Divergence::FixedArrayConstantIndexBoundsForm,
        Divergence::FixedArrayIndexNotIntForm,
        Divergence::ArrayIndexNotIntForm,
        Divergence::PrivateMemberReadForm,
        Divergence::YieldDelegationForm,
        Divergence::ConditionalNonBooleanConditionForm,
        Divergence::LogicalNonBooleanOperandForm,
        Divergence::LogicalNotNonBooleanForm,
        Divergence::DistinctNominalContainerAssignmentForm,
        Divergence::ConstructorTypeAnnotationForm,
        Divergence::IntersectionTypeAnnotationForm,
        Divergence::LibTypeName,
        Divergence::GeneratorYieldTypeMissingForm,
        Divergence::ArrayTypeArgument,
        Divergence::QualifiedSourceTypeNameForm,
        Divergence::FunctionReturnAnnotationMissingForm,
        Divergence::BlockLambdaReturnAnnotationMissingForm,
        Divergence::GenericClassDefaultParameterAnnotationNeeded,
        Divergence::FunctionValueParameterAnnotationNeeded,
        Divergence::GenericCallbackParameterAnnotationNeeded,
        Divergence::NamedImportModuleMissingForm,
        Divergence::NamespaceImportTargetMissingForm,
        Divergence::MirrorModuleDeclarationForm,
        Divergence::DuplicateLiteralAliasMemberForm,
        Divergence::SourceAliasNotLiteralUnionForm,
        Divergence::GenericSourceAliasForm,
        Divergence::EnumStringMemberNameForm,
        Divergence::ConstructorParameterPropertyForm,
        Divergence::FieldTypeWithoutInitializer,
        Divergence::IdentifierFieldName,
        Divergence::GeneratorMethodForm,
        Divergence::SwitchDiscriminantKindForm,
        Divergence::ForOfSpreadCall,
        Divergence::ForOfBindingKindForm,
        Divergence::ForOfVarBindingForm,
        Divergence::AsyncForOfForm,
        Divergence::StatementNonBooleanConditionForm,
        Divergence::GeneratorReturnValue,
        Divergence::LocalTypeWithoutInitializerForm,
        Divergence::LocalVarDeclarationForm,
        Divergence::ReturnFlowCoverage,
        Divergence::ModuleInitializerMissingForm,
        Divergence::ModuleVarDeclarationForm,
        Divergence::LibConstructorName,
        Divergence::GeneratorFunctionValueForm,
        Divergence::GenericSynchronousMethodValueForm,
        Divergence::SynchronousMethodValueForm,
        Divergence::MapCopyNullableSource,
        Divergence::ClassInheritedObjectMember,
        Divergence::ClassRuntimeObject,
        Divergence::UnaryNumericCoercion,
        Divergence::BitwiseIntegerOperand,
        Divergence::DeleteProperty,
        Divergence::OptionalMethodCall,
        Divergence::OptionalFunctionCall,
        Divergence::UndefinedEqualityPair,
        Divergence::UndefinedEqualityNonMember,
        Divergence::BareYieldNonVoid,
        Divergence::AsyncMethodValue,
        Divergence::GenericAsyncMethodValue,
        Divergence::FixedArrayObjectMember,
        Divergence::MapObjectMember,
        Divergence::SetObjectMember,
        Divergence::GeneratorResultObjectMember,
        Divergence::NumericObjectMember,
        Divergence::BoundaryObjectMember,
        Divergence::WorkerStaticObjectMethod,
        Divergence::WorkerObjectMethod,
        Divergence::InboxObjectMethod,
        Divergence::OutboxObjectMethod,
        Divergence::FixedArrayObjectMethod,
        Divergence::NumericObjectMethod,
        Divergence::StringObjectMember,
        Divergence::MapObjectMethod,
        Divergence::SetObjectMethod,
        Divergence::ArrayObjectMember,
        Divergence::RegexCompile,
        Divergence::FractionalIntegerLiteral,
        Divergence::FixedArrayLiteralLength,
        Divergence::ByteArgumentIdentity,
        Divergence::GenericConstructorTypeArguments,
        Divergence::WorkerSpawnSpread,
        Divergence::WorkerEntryLocalValue,
        Divergence::WorkerEntryGeneric,
        Divergence::WorkerEntrySignature,
        Divergence::WorkerEntryStructuralEndpoints,
        Divergence::WorkerExplicitMessageIdentity,
        Divergence::AwaitNonHandle,
        Divergence::AwaitUndeclaredAsyncFunction,
        Divergence::AwaitSynchronousFunction,
        Divergence::AwaitComputedMethod,
        Divergence::AwaitNonClassMethod,
        Divergence::AwaitSynchronousMethod,
        Divergence::AwaitIndirectCall,
        Divergence::ArrayUnshiftEmpty,
        Divergence::ArrayCallbackThisArgument,
        Divergence::MapCallbackThisArgument,
        Divergence::SetCallbackThisArgument,
        Divergence::MapGroupByArraySource,
        Divergence::MapGroupByVoidKey,
        Divergence::NullInitializerInference,
        Divergence::UsingBindingResourceType,
        Divergence::FunctionBodyMissing,
        Divergence::StaticFieldInitializerMissing,
        Divergence::FieldAssignmentAfterUnreachableReturn,
        Divergence::FieldAssignmentMissingNoNormalExit,
        Divergence::ConstructorFieldReadWithAssignmentFact,
        Divergence::ForOfAwaitUsing,
        Divergence::AliasCaseNonLiteral,
        Divergence::ClassMemberNameClash,
        Divergence::DescriptorMethod,
        Divergence::MirrorStaticMethod,
        Divergence::DisposeStatic,
        Divergence::MirrorAccessor,
        Divergence::ReadAccessorReturnMissing,
        Divergence::WriteAccessorPattern,
        Divergence::WriteAccessorTypeMissing,
        Divergence::GenericMethodBodyMissing,
        Divergence::DisposeAsync,
        Divergence::DisposeSignature,
        Divergence::DescriptorInheritance,
        Divergence::ReferenceClassInheritance,
        Divergence::DescriptorStaticField,
        Divergence::MirrorStaticField,
        Divergence::StaticFieldOptional,
        Divergence::ContextAffineStaticField,
        Divergence::DescriptorInitializerWithoutOptional,
        Divergence::DescriptorRequiredWithoutDefinite,
        Divergence::InstanceFieldOptional,
        Divergence::WireAliasNestedField,
        Divergence::ContextAffineInstanceField,
        Divergence::ValueFieldOutsideWhitelist,
        Divergence::DescriptorConstructor,
        Divergence::WireAliasNestedConstructorParameter,
        Divergence::ClassIndexSignatureCount,
        Divergence::ClassIndexSignatureNonReference,
        Divergence::ClassIndexSignatureStatic,
        Divergence::ClassIndexSignatureIndexType,
        Divergence::WriteAccessorWithoutRead,
        Divergence::AccessorTypeMismatch,
        Divergence::ClassIndexSetSignature,
        Divergence::ModuleUsing,
        Divergence::DescriptorOptions,
        Divergence::UnsupportedClassDecorator,
        Divergence::DescriptorValueType,
        Divergence::EnumImplicitValueOverflow,
        Divergence::WireEnumEmpty,
        Divergence::WireEnumMemberForm,
        Divergence::WireEnumMemberKey,
        Divergence::MirrorVariableForm,
        Divergence::WireAliasNestedForeignParameter,
        Divergence::WireAliasNestedForeignReturn,
        Divergence::ForeignDirectCallback,
        Divergence::ForeignReturnProvenance,
        Divergence::AsyncGeneratorFunction,
        Divergence::AsyncReturnAnnotationMissing,
        Divergence::OptionalParameter,
        Divergence::ErrorMessageType,
        Divergence::ErrorConstructorArguments,
        Divergence::ErrorCallWithoutNew,
        Divergence::ValueTypeArgumentCount,
        Divergence::ValueTypeOptionsNonLiteral,
        Divergence::ValueTypeOptionCount,
        Divergence::ValueTypeOptionSpread,
        Divergence::ValueTypeOptionPropertyForm,
        Divergence::ValueTypeOptionKey,
        Divergence::ValueTypeAlignmentNonLiteral,
        Divergence::ValueTypeAlignmentOutsideSet,
        Divergence::ComputedMethodName,
        Divergence::MirrorExportList,
        Divergence::TopLevelNameClash,
        Divergence::UnsupportedModuleDeclaration,
        Divergence::PoisonedDefaultImport,
        Divergence::DefaultImport,
        Divergence::InvalidProgramEntry,
        Divergence::AsyncReturnNonReference,
        Divergence::AsyncReturnQualifiedName,
        Divergence::AsyncReturnAlias,
        Divergence::AsyncReturnMissingArgument,
        Divergence::AsyncReturnArgumentCount,
        Divergence::FixedArrayLengthRange,
        Divergence::FixedArrayLengthLiteral,
        Divergence::GenericConstraintIdentity,
        Divergence::NamespaceUnexportedMember,
        Divergence::RunnerMainMissing,
        Divergence::ClassFinalAlignmentLimit,
        Divergence::AggregateArgumentFrameLimit,
        Divergence::NamespaceAsValue,
        Divergence::SwitchCaseClosureRead,
        Divergence::BlockNameReadBeforeDeclaration,
        Divergence::BlockNameWriteBeforeDeclaration,
        Divergence::ContextAffineCapture,
        Divergence::MutableLocalCapture,
        Divergence::ContextAffineArrayElement,
        Divergence::ContextAffineContainerArgument,
        Divergence::WorkerMessagePlainClass,
        Divergence::BoundaryLiteralAlias,
        Divergence::NullableNonReference,
        Divergence::NullableValueClassAssignment,
        Divergence::MirrorHeaderMissing,
        Divergence::MirrorParameterTargetMissing,
        Divergence::MirrorCallbackTargetMissing,
        Divergence::MirrorLifetimeTargetMissing,
        Divergence::MirrorArrayProvenanceMissing,
        Divergence::MirrorStringProvenanceMissing,
        Divergence::MirrorParameterProvenanceMismatch,
        Divergence::MirrorAnonymousCallback,
        Divergence::MirrorCallbackProvenanceMissing,
        Divergence::ProvenanceUnknownKind,
        Divergence::ProvenanceMissingKind,
        Divergence::ProvenanceFieldSeparator,
        Divergence::ProvenanceUnexpectedKey,
        Divergence::ProvenanceUnquotedString,
        Divergence::ProvenanceUnterminatedString,
        Divergence::ProvenanceUnterminatedEscape,
        Divergence::ProvenanceUnsupportedEscape,
        Divergence::ProvenanceControlCharacter,
        Divergence::ProvenanceInvalidUnicodeDigits,
        Divergence::ProvenanceInvalidBoolean,
        Divergence::ProvenanceTrailingData,
        Divergence::ProvenanceShortUnicodeEscape,
        Divergence::ProvenanceInvalidUnicodeScalar,
        Divergence::ProvenanceHeaderBasename,
        Divergence::ProvenanceDuplicateHeader,
        Divergence::ProvenanceEmptyDescriptor,
        Divergence::ProvenanceDuplicateDescriptor,
        Divergence::ProvenanceEmptyStringView,
        Divergence::ProvenanceDuplicateStringView,
        Divergence::ProvenanceEmptyScalarPair,
        Divergence::ProvenanceDuplicateScalarPair,
        Divergence::ProvenanceEmptyCallback,
        Divergence::ProvenanceDuplicateCallback,
        Divergence::ProvenanceEmptyCallbackLifetime,
        Divergence::ProvenanceDuplicateCallbackLifetime,
        Divergence::ProvenanceEmptyExternalType,
        Divergence::ProvenanceDuplicateExternalType,
        Divergence::ProvenanceEmptyCEnum,
        Divergence::ProvenanceDuplicateCEnum,
        Divergence::IterationSubjectDomain,
        Divergence::FixedArrayMethods,
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
        Divergence::VoidValue,
        Divergence::ReferenceSearchMiss,
        Divergence::GeneratorDoneValue,
        Divergence::AnyType,
        Divergence::DynamicObjectModel,
        Divergence::NominalClassIdentity,
        Divergence::ObjectLiteralConstruction,
        Divergence::ValueClassLayout,
        Divergence::BareNumber,
        Divergence::SizedOperandWidths,
        Divergence::StorageOnlyHalfFloat,
        Divergence::IntegerLiteralRange,
        Divergence::WireEnumValues,
        Divergence::EscapingCapture,
        Divergence::Exceptions,
        Divergence::InstanceofNonError,
        Divergence::GeneralUnionAndUndefined,
        Divergence::NullishNonNullable,
        Divergence::OptionalChainNonNullable,
        Divergence::NullishAssignment,
        Divergence::NonPlaceNullishInitializer,
        Divergence::OptionalChainUnbound,
        Divergence::OptionalChainIndex,
        Divergence::LiteralUnionAlias,
        Divergence::OptionalDescriptorMember,
        Divergence::BoundaryOnlyObject,
        Divergence::PromiseAllVoidValue,
        Divergence::PromiseAllTypeArguments,
        Divergence::PromiseAllInput,
        Divergence::PromiseAllCountedResult,
        Divergence::PromiseObject,
        Divergence::AwaitOutsideAsync,
        Divergence::AsyncFunctionShape,
        Divergence::AsyncArrowCapture,
        Divergence::AsyncReturnHandle,
        Divergence::DroppedAsyncHandle,
        Divergence::ThisInFieldInitializer,
        Divergence::ClassIndexSignature,
        Divergence::UsingDeclaration,
        Divergence::NamedAccessor,
        Divergence::IteratorTemporary,
        Divergence::DeclarationScope,
        Divergence::NamedModuleSurface,
        Divergence::HostApiSurface,
        Divergence::ModuleInitializerOrder,
        Divergence::GrowingInstanceChain,
        Divergence::StaticMemberSurface,
        Divergence::MathSubset,
        Divergence::DateSubset,
        Divergence::LocaleSensitiveString,
        Divergence::ArrayMethodDefaults,
        Divergence::VariadicArguments,
        Divergence::MapKeyKind,
        Divergence::MapScalarGet,
        Divergence::MapNonNullableGet,
        Divergence::SharedLocationNarrowing,
        Divergence::NoTupleType,
        Divergence::NumberCoercionAndArguments,
        Divergence::JsonSubset,
        Divergence::AggregateLayoutLimit,
        Divergence::LoneSurrogateEscape,
        Divergence::RegExpSubset,
        Divergence::ReplaceAllGlobalFlag,
        Divergence::WorkerEntryShape,
        Divergence::WorkerContextAffinity,
        Divergence::SwitchOverAlias,
        Divergence::UnreachableInValuePosition,
        Divergence::DescriptorConstruction,
        Divergence::ByteAccessTarget,
        Divergence::EntryParameterType,
        Divergence::EmbeddedHeaderCopy,
        Divergence::GenericInferenceCandidates,
        Divergence::GenericInferenceMissing,
        Divergence::GenericMethodTypeArguments,
        Divergence::BodilessDeclareGenericMethod,
        Divergence::GenericMethodOnGenericClass,
        Divergence::GeneratorSingleUse,
        Divergence::BareMapSubject,
        Divergence::BareMapToArray,
        Divergence::ArrayFromMapper,
        Divergence::ArrayIsArray,
        Divergence::ArrayOfArity,
        Divergence::ArrayHoleConstruction,
        Divergence::PatternDefaultValue,
        Divergence::ArrayRestPattern,
        Divergence::ObjectRestPattern,
        Divergence::NestedPattern,
        Divergence::PatternFieldName,
        Divergence::PatternSourceShape,
        Divergence::AssignmentPattern,
        Divergence::ModuleLevelPattern,
        Divergence::DefiniteAssignmentAssertion,
        Divergence::NestedFieldAssignmentEveryNormalExit,
        Divergence::AmbientClassConstruction,
        Divergence::ThisBeforeFieldValues,
    ];
}

mod entries;
mod type_flow;

#[cfg(test)]
mod tests {
    use super::{Divergence, DivergenceEntry};
    use std::collections::BTreeSet;

    /// The collision record, read at compile time (§79 rule 5).
    const COLLISIONS: &str = include_str!("../../specs/blocks/collisions.md");

    /// This source file, read at compile time. The variant names come
    /// from the enum text, so the count of `ALL` compares against a fact
    /// that the table itself did not produce (CLAUDE.md principle 9).
    const SOURCE: &str = include_str!("divergence.rs");

    /// Every collision heading that requires a rejection diagnostic.
    fn recorded_headings() -> BTreeSet<String> {
        diagnostic_headings(COLLISIONS)
    }

    fn diagnostic_headings(source: &str) -> BTreeSet<String> {
        let mut headings = BTreeSet::new();
        let mut current = None;
        for line in source.lines() {
            if line.starts_with("## ") || line.starts_with("### ") {
                current = line
                    .strip_prefix("### ")
                    .and_then(|rest| rest.split('.').next())
                    .filter(|id| {
                        id.starts_with('C')
                            && id.len() > 1
                            && id[1..].chars().all(|c| c.is_ascii_digit())
                    });
                if let Some(id) = current {
                    headings.insert(id.to_owned());
                }
            } else if line == "No diagnostic reports this." {
                if let Some(id) = current {
                    headings.remove(id);
                }
            }
        }
        headings
    }

    #[test]
    fn only_an_explicit_standalone_marker_exempts_its_collision_heading() {
        let source = "### C1. Reject\nText: No diagnostic reports this.\n\n### C2. Behavior\nNo diagnostic reports this.\n\n### C3. Reject\nAccept: `a5`.\n\n## 2. Other records\nNo diagnostic reports this.\n";
        assert_eq!(
            diagnostic_headings(source),
            BTreeSet::from(["C1".to_owned(), "C3".to_owned()])
        );
    }

    /// The variant names declared in the `Divergence` enum body.
    fn declared_variants() -> BTreeSet<String> {
        let start = SOURCE
            .find("pub enum Divergence {\n")
            .expect("Divergence enum")
            + "pub enum Divergence {\n".len();
        let body = &SOURCE[start..];
        let end = body.find("\n}\n").expect("the end of the enum body");
        body[..end]
            .lines()
            .map(str::trim)
            .filter(|line| line.ends_with(','))
            .map(|line| line.trim_end_matches(',').to_owned())
            .filter(|name| {
                name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                    && name.chars().all(|c| c.is_ascii_alphanumeric())
            })
            .collect()
    }

    /// A collision id that is not a `C<n>` heading names the section that
    /// decided the rule. These are the accepted spellings.
    fn is_section_id(id: &str) -> bool {
        for file in ["compiler.md §", "stdlib.md §", "collisions.md §"] {
            if let Some(rest) = id.strip_prefix(file) {
                return !rest.is_empty()
                    && rest.chars().all(|c| c.is_ascii_digit() || c == '.')
                    && rest.starts_with(|c: char| c.is_ascii_digit());
            }
        }
        if let Some(rest) = id.strip_prefix("collisions.md Q") {
            return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
        }
        false
    }

    #[test]
    fn all_lists_every_variant_one_time() {
        let declared = declared_variants();
        assert!(
            !declared.is_empty(),
            "the enum body gave no variant names; the reader is wrong"
        );

        let listed: BTreeSet<String> = Divergence::ALL.iter().map(|d| format!("{d:?}")).collect();

        let missing: Vec<&String> = declared.difference(&listed).collect();
        assert!(missing.is_empty(), "variants absent from ALL: {missing:?}");

        let unknown: Vec<&String> = listed.difference(&declared).collect();
        assert!(unknown.is_empty(), "ALL names no such variant: {unknown:?}");

        assert_eq!(
            Divergence::ALL.len(),
            listed.len(),
            "ALL lists a variant more than one time"
        );
    }

    #[test]
    fn collision_ids_and_headings_are_total() {
        let mut headings = recorded_headings();
        // §99 retains C15's heading for links but retires its diagnostic.
        assert!(
            headings.remove("C15"),
            "the retired C15 heading must remain"
        );
        let c15 = COLLISIONS
            .split("### C15.")
            .nth(1)
            .expect("C15 exists")
            .split("### ")
            .next()
            .expect("C15 has a body");
        assert!(
            c15.lines().any(|line| {
                line == "Accept: `a183`. Reject: retired:r184-string-literal-too-long."
            }),
            "C15 must record its retired witness"
        );
        let mut bad: Vec<String> = Vec::new();
        for divergence in Divergence::ALL {
            let id = divergence.entry().collision;
            let known = if id.starts_with('C') && id[1..].chars().all(|c| c.is_ascii_digit()) {
                headings.contains(id)
            } else {
                is_section_id(id)
            };
            if !known {
                bad.push(format!("{divergence:?} cites `{id}`"));
            }
        }
        assert!(bad.is_empty(), "unrecorded collision ids: {bad:#?}");

        let cited: BTreeSet<&str> = Divergence::ALL
            .iter()
            .map(|d| d.entry().collision)
            .collect();
        let missing: Vec<String> = headings
            .into_iter()
            .filter(|id| !cited.contains(id.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "collisions.md headings with no variant: {missing:?}"
        );
    }

    #[test]
    fn generator_done_value_names_collision_c23() {
        let entry = Divergence::GeneratorDoneValue.entry();
        assert_eq!(entry.collision, "C23");
        assert!(entry.subscript.contains("!r.done"));
        assert!(Divergence::ALL.contains(&Divergence::GeneratorDoneValue));
    }

    #[test]
    fn fragments_differ_and_reasons_are_short() {
        let mut same: Vec<String> = Vec::new();
        for divergence in Divergence::ALL {
            let DivergenceEntry { ts, subscript, .. } = divergence.entry();
            if ts == subscript {
                same.push(format!("{divergence:?}"));
            }
        }
        assert!(
            same.is_empty(),
            "the TypeScript and subscript fragments are equal: {same:?}"
        );

        let mut long: Vec<String> = Vec::new();
        for divergence in Divergence::ALL {
            let why = divergence.entry().why;
            let words = why.split_whitespace().count();
            if words > 25 {
                long.push(format!("{divergence:?}: {words} words"));
            }
        }
        assert!(
            long.is_empty(),
            "a reason is longer than 25 words: {long:?}"
        );
    }

    #[test]
    fn generic_inference_names_its_rule_and_explicit_fix() {
        let divergence = Divergence::GenericInferenceCandidates;
        let entry = divergence.entry();
        assert_eq!(entry.collision, "compiler.md §149");
        assert!(entry.ts.contains("pair(n, f)"));
        assert!(entry.subscript.contains("pair<f64>(n as f64, f)"));
        assert!(Divergence::ALL.contains(&divergence));
    }

    #[test]
    fn missing_inference_candidate_has_its_own_example() {
        let divergence = Divergence::GenericInferenceMissing;
        let entry = divergence.entry();
        assert_eq!(entry.collision, "compiler.md §149");
        assert_eq!(
            entry.ts,
            "function empty<T>(): T | null { return null; }\nempty();"
        );
        assert_eq!(
            entry.subscript,
            "class Item {}\nfunction empty<T>(): T | null { return null; }\nempty<Item>();"
        );
        assert!(Divergence::ALL.contains(&divergence));
    }

    #[test]
    fn void_value_names_its_record_and_statement_form() {
        let entry = Divergence::VoidValue.entry();
        assert_eq!(entry.collision, "C21");
        assert_eq!(entry.ts, "function f(): void {} const a = f();");
        assert_eq!(entry.subscript, "function f(): void {} f();");
        assert!(Divergence::ALL.contains(&Divergence::VoidValue));
    }

    #[test]
    fn reference_search_miss_names_its_record_and_null_test() {
        let entry = Divergence::ReferenceSearchMiss.entry();
        assert_eq!(entry.collision, "C22");
        assert_eq!(
            entry.ts,
            "const missing = values.get(key); print(`${missing === undefined}`);"
        );
        assert_eq!(
            entry.subscript,
            "const missing = values.get(key); print(`${missing == null}`);"
        );
        assert!(Divergence::ALL.contains(&Divergence::ReferenceSearchMiss));
    }

    #[test]
    fn every_fragment_has_content() {
        for divergence in Divergence::ALL {
            let entry = divergence.entry();
            assert!(!entry.ts.is_empty(), "{divergence:?} has no `ts` fragment");
            assert!(
                !entry.subscript.is_empty(),
                "{divergence:?} has no `subscript` fragment"
            );
            assert!(!entry.why.is_empty(), "{divergence:?} has no reason");
            assert!(
                !entry.collision.is_empty(),
                "{divergence:?} has no collision id"
            );
        }
    }
}

mod promise_all;
