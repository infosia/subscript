//! Scalar, string, regular-expression, and JSON intrinsics.

use super::*;

impl Interpreter<'_> {
    pub(super) fn intrinsic_ambient(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
    ) -> Result<Value, InterpretError> {
        match operation {
            "Print" => {
                let string = operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "print has no argument"))?
                    .as_handle()?;
                // SAFETY: live runtime string.
                unsafe { ffi::subscript_rt_print(&mut *self.context, string) };
                Ok(Value::Void)
            }
            "UnsafeDelete" => {
                let handle = operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "Context.free has no argument"))?
                    .as_handle()?;
                self.context.delete(handle as usize, 0);
                Ok(Value::Void)
            }
            "Unreachable" => {
                self.context.trap(
                    RuntimeTrapKind::UnreachableReached,
                    "execution reached unreachable()",
                    0,
                );
                Ok(Value::Void)
            }
            _ => Err(self.invalid(None, format!("unknown Ambient intrinsic {operation}"))),
        }
    }

    pub(super) fn intrinsic_context_bytes(
        &mut self,
        intrinsic: &l::Intrinsic,
        operation: &str,
        operands: Vec<Value>,
        pos: Option<&Pos>,
    ) -> Result<Value, InterpretError> {
        let ty = intrinsic.type_argument.as_ref().ok_or_else(|| {
            self.invalid(
                pos.cloned(),
                format!("Context.{operation} has no type argument"),
            )
        })?;
        let layout = self.layout_cached(ty).ok_or_else(|| {
            self.invalid(
                pos.cloned(),
                format!("Context.{operation} type has no storage layout"),
            )
        })?;
        let size = u32::try_from(layout.size).map_err(|_| {
            self.invalid(
                pos.cloned(),
                format!("Context.{operation} storage size exceeds u32"),
            )
        })?;
        let trap_pos = pos
            .cloned()
            .unwrap_or_else(|| Pos::new("<Context bytes>", 1, 1));
        match operation {
            "BytesOf" => {
                let value = operands
                    .first()
                    .ok_or_else(|| self.invalid(pos.cloned(), "Context.BytesOf has no value"))?;
                let mut bytes = self.pack(ty, value)?;
                self.zero_padding(ty, &mut bytes, 0)?;
                let handle = unsafe {
                    ffi::subscript_rt_array_from_bytes(&mut *self.context, bytes.as_ptr(), size, 0)
                };
                self.check_runtime(&trap_pos)?;

                Ok(Value::Handle(handle))
            }
            "BytesInto" => {
                let value = operands
                    .first()
                    .ok_or_else(|| self.invalid(pos.cloned(), "Context.BytesInto has no value"))?;
                let target = operands
                    .get(1)
                    .ok_or_else(|| self.invalid(pos.cloned(), "Context.BytesInto has no target"))?
                    .as_handle()?;
                let offset = u32::try_from(
                    operands
                        .get(2)
                        .ok_or_else(|| {
                            self.invalid(pos.cloned(), "Context.BytesInto has no offset")
                        })?
                        .as_u64()?,
                )
                .map_err(|_| self.invalid(pos.cloned(), "Context.BytesInto offset exceeds u32"))?;
                let mut bytes = self.pack(ty, value)?;
                self.zero_padding(ty, &mut bytes, 0)?;
                let range = unsafe {
                    ffi::subscript_rt_array_byte_range(&mut *self.context, target, offset, size, 0)
                };
                self.check_runtime(&trap_pos)?;
                if size != 0 {
                    if range.is_null() {
                        return Err(self.invalid(pos.cloned(), "Context.BytesInto returned null"));
                    }
                    unsafe {
                        std::ptr::copy_nonoverlapping(bytes.as_ptr(), range, layout.size);
                    }
                }
                Ok(Value::Void)
            }
            "FromBytes" => {
                let source = operands
                    .first()
                    .ok_or_else(|| self.invalid(pos.cloned(), "Context.FromBytes has no source"))?
                    .as_handle()?;
                let offset = u32::try_from(
                    operands
                        .get(1)
                        .ok_or_else(|| {
                            self.invalid(pos.cloned(), "Context.FromBytes has no offset")
                        })?
                        .as_u64()?,
                )
                .map_err(|_| self.invalid(pos.cloned(), "Context.FromBytes offset exceeds u32"))?;
                let range = unsafe {
                    ffi::subscript_rt_array_byte_range(&mut *self.context, source, offset, size, 0)
                };
                self.check_runtime(&trap_pos)?;
                if size != 0 && range.is_null() {
                    return Err(self.invalid(pos.cloned(), "Context.FromBytes returned null"));
                }
                if layout.size == 0 {
                    return self.unpack(ty, &[]);
                }
                let bytes = unsafe { std::slice::from_raw_parts(range, layout.size) };
                self.unpack(ty, bytes)
            }
            _ => Err(self.invalid(
                pos.cloned(),
                format!("unknown Context byte intrinsic {operation}"),
            )),
        }
    }

    pub(super) fn intrinsic_math(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
    ) -> Result<Value, InterpretError> {
        let unary = |function: fn(f64) -> f64| -> Result<Value, InterpretError> {
            Ok(Value::F64(function(
                operands
                    .first()
                    .ok_or_else(|| self.invalid(None, format!("Math.{operation} has no operand")))?
                    .as_f64()?,
            )))
        };
        let binary = |function: fn(f64, f64) -> f64| -> Result<Value, InterpretError> {
            Ok(Value::F64(function(
                operands
                    .first()
                    .ok_or_else(|| {
                        self.invalid(None, format!("Math.{operation} has no left operand"))
                    })?
                    .as_f64()?,
                operands
                    .get(1)
                    .ok_or_else(|| {
                        self.invalid(None, format!("Math.{operation} has no right operand"))
                    })?
                    .as_f64()?,
            )))
        };
        match operation {
            "Abs" => unary(subscript_runtime::math::abs),
            "Acos" => unary(subscript_runtime::math::acos),
            "Acosh" => unary(subscript_runtime::math::acosh),
            "Asin" => unary(subscript_runtime::math::asin),
            "Asinh" => unary(subscript_runtime::math::asinh),
            "Atan" => unary(subscript_runtime::math::atan),
            "Atanh" => unary(subscript_runtime::math::atanh),
            "Cbrt" => unary(subscript_runtime::math::cbrt),
            "Ceil" => unary(subscript_runtime::math::ceil),
            "Cos" => unary(subscript_runtime::math::cos),
            "Cosh" => unary(subscript_runtime::math::cosh),
            "Exp" => unary(subscript_runtime::math::exp),
            "Expm1" => unary(subscript_runtime::math::expm1),
            "Floor" => unary(subscript_runtime::math::floor),
            "Log" => unary(subscript_runtime::math::log),
            "Log1p" => unary(subscript_runtime::math::log1p),
            "Log10" => unary(subscript_runtime::math::log10),
            "Log2" => unary(subscript_runtime::math::log2),
            "Round" => unary(subscript_runtime::math::round),
            "Sign" => unary(subscript_runtime::math::sign),
            "Sin" => unary(subscript_runtime::math::sin),
            "Sinh" => unary(subscript_runtime::math::sinh),
            "Sqrt" => unary(subscript_runtime::math::sqrt),
            "Tan" => unary(subscript_runtime::math::tan),
            "Tanh" => unary(subscript_runtime::math::tanh),
            "Trunc" => unary(subscript_runtime::math::trunc),
            "Atan2" => binary(subscript_runtime::math::atan2),
            "Hypot" => binary(subscript_runtime::math::hypot),
            "Pow" => binary(subscript_runtime::math::pow),
            "Max" => binary(subscript_runtime::math::max),
            "Min" => binary(subscript_runtime::math::min),
            "Random" => {
                // SAFETY: exclusively owned runtime Context.
                Ok(Value::F64(unsafe {
                    ffi::subscript_rt_math_random(&mut *self.context)
                }))
            }
            "Clz32" => Ok(Value::I(subscript_runtime::math::clz32(
                operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "Math.clz32 has no operand"))?
                    .as_u64()? as u32,
            ) as i64)),
            "Imul" => Ok(Value::I(subscript_runtime::math::imul(
                operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "Math.imul has no left operand"))?
                    .as_i64()? as i32,
                operands
                    .get(1)
                    .ok_or_else(|| self.invalid(None, "Math.imul has no right operand"))?
                    .as_i64()? as i32,
            ) as i64)),
            "Fround" => Ok(Value::F64(subscript_runtime::math::fround(
                operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "Math.fround has no operand"))?
                    .as_f64()?,
            ))),
            "F32ToBits" => Ok(Value::U(subscript_runtime::math::f32_to_bits(
                operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "Math.f32ToBits has no operand"))?
                    .as_f64()?,
            ) as u64)),
            "F32FromBits" => Ok(Value::F64(subscript_runtime::math::f32_from_bits(
                operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "Math.f32FromBits has no operand"))?
                    .as_u64()? as u32,
            ))),
            _ => Err(self.invalid(None, format!("unknown Math intrinsic {operation}"))),
        }
    }

    pub(super) fn intrinsic_number(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
    ) -> Result<Value, InterpretError> {
        let context = &mut *self.context as *mut Context;
        let first = || {
            operands.first().ok_or_else(|| {
                self.invalid(None, format!("Number.{operation} has no first operand"))
            })
        };
        let second = || {
            operands.get(1).ok_or_else(|| {
                self.invalid(None, format!("Number.{operation} has no second operand"))
            })
        };
        let value = match operation {
            // SAFETY: pure runtime predicates with the interpreter's Context.
            "IsNaN" => Value::Bool(
                unsafe { ffi::subscript_rt_num_is_nan(context, first()?.as_f64()?) } != 0,
            ),
            // SAFETY: pure runtime predicates with the interpreter's Context.
            "IsFinite" => Value::Bool(
                unsafe { ffi::subscript_rt_num_is_finite(context, first()?.as_f64()?) } != 0,
            ),
            // SAFETY: pure runtime predicates with the interpreter's Context.
            "IsInteger" => Value::Bool(
                unsafe { ffi::subscript_rt_num_is_integer(context, first()?.as_f64()?) } != 0,
            ),
            // SAFETY: pure runtime predicates with the interpreter's Context.
            "IsSafeInteger" => Value::Bool(
                unsafe { ffi::subscript_rt_num_is_safe_integer(context, first()?.as_f64()?) } != 0,
            ),
            // SAFETY: live runtime string and explicit radix.
            "ParseInt" => Value::F64(unsafe {
                ffi::subscript_rt_num_parse_int(
                    context,
                    first()?.as_handle()?,
                    second()?.as_i64()? as i32,
                    0,
                )
            }),
            // SAFETY: live runtime string.
            "ParseFloat" => Value::F64(unsafe {
                ffi::subscript_rt_num_parse_float(context, first()?.as_handle()?, 0)
            }),
            // SAFETY: scalar arguments; runtime owns formatting/range checks.
            "ToFixed" => Value::Handle(unsafe {
                ffi::subscript_rt_num_to_fixed(
                    context,
                    first()?.as_f64()?,
                    second()?.as_i64()? as i32,
                    0,
                )
            }),
            // SAFETY: scalar arguments; runtime owns formatting/range checks.
            "ToStringF32" => Value::Handle(unsafe {
                ffi::subscript_rt_num_to_string_f32(
                    context,
                    first()?.as_f64()? as f32,
                    second()?.as_i64()? as i32,
                    0,
                )
            }),
            // SAFETY: scalar arguments; runtime owns formatting/range checks.
            "ToStringF64" => Value::Handle(unsafe {
                ffi::subscript_rt_num_to_string_f64(
                    context,
                    first()?.as_f64()?,
                    second()?.as_i64()? as i32,
                    0,
                )
            }),
            // SAFETY: scalar arguments; runtime owns formatting/range checks.
            "ToExponential" => Value::Handle(unsafe {
                ffi::subscript_rt_num_to_exponential(
                    context,
                    first()?.as_f64()?,
                    second()?.as_i64()? as i32,
                    0,
                )
            }),
            // SAFETY: scalar arguments; runtime owns formatting/range checks.
            "ToPrecision" => Value::Handle(unsafe {
                ffi::subscript_rt_num_to_precision(
                    context,
                    first()?.as_f64()?,
                    second()?.as_i64()? as i32,
                    0,
                )
            }),
            _ => return Err(self.invalid(None, format!("unknown Number intrinsic {operation}"))),
        };
        self.check_runtime(&Pos::new("<number>", 1, 1))?;
        if let Value::Handle(handle) = value {
            Ok(Value::Handle(handle))
        } else {
            Ok(value)
        }
    }

    pub(super) fn intrinsic_date(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
    ) -> Result<Value, InterpretError> {
        let context = &mut *self.context as *mut Context;
        let first = || {
            operands
                .first()
                .ok_or_else(|| self.invalid(None, format!("Date.{operation} has no receiver")))
        };
        let value =
            match operation {
                // SAFETY: scalar Date representation and owned Context.
                "New" => {
                    Value::I(unsafe { ffi::subscript_rt_date_new(context, first()?.as_i64()?, 0) })
                }
                "Utc" => {
                    if operands.len() != 7 {
                        return Err(self
                            .invalid(None, format!("Date.UTC has {} arguments", operands.len())));
                    }
                    // SAFETY: seven checker-normalized scalar components.
                    Value::I(unsafe {
                        ffi::subscript_rt_date_utc(
                            context,
                            operands[0].as_i64()? as i32,
                            operands[1].as_i64()? as i32,
                            operands[2].as_i64()? as i32,
                            operands[3].as_i64()? as i32,
                            operands[4].as_i64()? as i32,
                            operands[5].as_i64()? as i32,
                            operands[6].as_i64()? as i32,
                            0,
                        )
                    })
                }
                // SAFETY: owned Context clock.
                "Now" => Value::I(unsafe { ffi::subscript_rt_date_now(context) }),
                "GetUtcFullYear" | "GetUtcMonth" | "GetUtcDate" | "GetUtcDay" | "GetUtcHours"
                | "GetUtcMinutes" | "GetUtcSeconds" | "GetUtcMilliseconds" => {
                    let field = match operation {
                        "GetUtcFullYear" => 0,
                        "GetUtcMonth" => 1,
                        "GetUtcDate" => 2,
                        "GetUtcDay" => 3,
                        "GetUtcHours" => 4,
                        "GetUtcMinutes" => 5,
                        "GetUtcSeconds" => 6,
                        _ => 7,
                    };
                    // SAFETY: valid Date field code.
                    Value::I(unsafe {
                        ffi::subscript_rt_date_get(context, first()?.as_i64()?, field)
                    } as i64)
                }
                // SAFETY: scalar Date and owned Context.
                "ToUtcString" => Value::Handle(unsafe {
                    ffi::subscript_rt_date_to_utc_string(context, first()?.as_i64()?, 0)
                }),
                // SAFETY: scalar Date; runtime owns ISO formatting/range checks.
                "ToIso" => Value::Handle(unsafe {
                    ffi::subscript_rt_date_to_iso(context, first()?.as_i64()?, 0)
                }),
                _ => return Err(self.invalid(None, format!("unknown Date intrinsic {operation}"))),
            };
        self.check_runtime(&Pos::new("<date>", 1, 1))?;
        if let Value::Handle(handle) = value {
            Ok(Value::Handle(handle))
        } else {
            Ok(value)
        }
    }

    pub(super) fn intrinsic_string(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
    ) -> Result<Value, InterpretError> {
        let receiver = operands
            .first()
            .ok_or_else(|| self.invalid(None, format!("String.{operation} has no receiver")))?
            .as_handle()?;
        let context = &mut *self.context as *mut Context;
        let handle = |index: usize| -> Result<*mut u8, InterpretError> {
            operands
                .get(index)
                .ok_or_else(|| {
                    self.invalid(None, format!("String.{operation} has no operand {index}"))
                })?
                .as_handle()
        };
        let integer = |index: usize| -> Result<i32, InterpretError> {
            Ok(operands
                .get(index)
                .ok_or_else(|| {
                    self.invalid(None, format!("String.{operation} has no operand {index}"))
                })?
                .as_i64()? as i32)
        };
        let value = match operation {
            // SAFETY: live strings and checker-normalized scalar arguments.
            "Slice" => Value::Handle(unsafe {
                ffi::subscript_rt_str_slice(context, receiver, integer(1)?, integer(2)?, 0)
            }),
            // SAFETY: live strings and scalar byte position.
            "IndexOf" => Value::I(unsafe {
                ffi::subscript_rt_str_index_of(context, receiver, handle(1)?, integer(2)?)
            } as i64),
            // SAFETY: live strings.
            "LastIndexOf" => Value::I(unsafe {
                ffi::subscript_rt_str_last_index_of(context, receiver, handle(1)?, integer(2)?)
            } as i64),
            // SAFETY: live strings and scalar byte position.
            "Includes" => Value::Bool(
                unsafe {
                    ffi::subscript_rt_str_includes(context, receiver, handle(1)?, integer(2)?)
                } != 0,
            ),
            // SAFETY: live strings and scalar byte position.
            "StartsWith" => Value::Bool(
                unsafe {
                    ffi::subscript_rt_str_starts_with(context, receiver, handle(1)?, integer(2)?)
                } != 0,
            ),
            // SAFETY: live strings and scalar byte position.
            "EndsWith" => Value::Bool(
                unsafe {
                    ffi::subscript_rt_str_ends_with(context, receiver, handle(1)?, integer(2)?)
                } != 0,
            ),
            // SAFETY: live string; runtime owns range check.
            "CharCodeAt" => Value::I(unsafe {
                ffi::subscript_rt_str_char_code_at(context, receiver, integer(1)?, 0)
            } as i64),
            // SAFETY: live strings; runtime owns split allocation.
            "Split" => Value::Handle(unsafe {
                ffi::subscript_rt_str_split(context, receiver, handle(1)?, integer(2)?, 0)
            }),
            // SAFETY: live string; runtime owns Unicode trimming.
            "Trim" => Value::Handle(unsafe { ffi::subscript_rt_str_trim(context, receiver, 0) }),
            // SAFETY: live string; runtime owns Unicode trimming.
            "TrimStart" => {
                Value::Handle(unsafe { ffi::subscript_rt_str_trim_start(context, receiver, 0) })
            }
            // SAFETY: live string; runtime owns Unicode trimming.
            "TrimEnd" => {
                Value::Handle(unsafe { ffi::subscript_rt_str_trim_end(context, receiver, 0) })
            }
            // SAFETY: live string; runtime owns range/allocation.
            "Repeat" => Value::Handle(unsafe {
                ffi::subscript_rt_str_repeat(context, receiver, integer(1)?, 0)
            }),
            // SAFETY: live strings; runtime owns byte padding.
            "PadStart" => Value::Handle(unsafe {
                ffi::subscript_rt_str_pad_start(context, receiver, integer(1)?, handle(2)?, 0)
            }),
            // SAFETY: live strings; runtime owns byte padding.
            "PadEnd" => Value::Handle(unsafe {
                ffi::subscript_rt_str_pad_end(context, receiver, integer(1)?, handle(2)?, 0)
            }),
            // SAFETY: live string; runtime owns Unicode conversion.
            "ToUpperCase" => {
                Value::Handle(unsafe { ffi::subscript_rt_str_to_upper(context, receiver, 0) })
            }
            // SAFETY: live string; runtime owns Unicode conversion.
            "ToLowerCase" => {
                Value::Handle(unsafe { ffi::subscript_rt_str_to_lower(context, receiver, 0) })
            }
            // SAFETY: live strings; runtime owns replacement semantics.
            "Replace" => Value::Handle(unsafe {
                ffi::subscript_rt_str_replace(context, receiver, handle(1)?, handle(2)?, 0)
            }),
            // SAFETY: live strings; runtime owns replacement semantics.
            "ReplaceAll" => Value::Handle(unsafe {
                ffi::subscript_rt_str_replace_all(context, receiver, handle(1)?, handle(2)?, 0)
            }),
            // SAFETY: live string; runtime owns byte-boundary rules.
            "Substring" => Value::Handle(unsafe {
                ffi::subscript_rt_str_substring(context, receiver, integer(1)?, integer(2)?, 0)
            }),
            // SAFETY: live string; runtime owns byte-boundary rules.
            "Substr" => Value::Handle(unsafe {
                ffi::subscript_rt_str_substr(context, receiver, integer(1)?, integer(2)?, 0)
            }),
            // SAFETY: live string; runtime owns byte-boundary rules.
            "At" => Value::Handle(unsafe {
                ffi::subscript_rt_str_at(context, receiver, integer(1)?, 0)
            }),
            // SAFETY: live string; runtime owns byte-boundary rules.
            "CharAt" => Value::Handle(unsafe {
                ffi::subscript_rt_str_char_at(context, receiver, integer(1)?, 0)
            }),
            // SAFETY: live string; runtime owns byte-boundary rules.
            "CodePointAt" => Value::I(unsafe {
                ffi::subscript_rt_str_code_point_at(context, receiver, integer(1)?, 0)
            } as i64),
            // SAFETY: live strings; shared concat implementation.
            "Concat" => Value::Handle(unsafe {
                ffi::subscript_rt_str_concat(context, receiver, handle(1)?, 0)
            }),
            _ => return Err(self.invalid(None, format!("unknown String intrinsic {operation}"))),
        };
        self.check_runtime(&Pos::new("<string>", 1, 1))?;
        if let Value::Handle(handle) = value {
            Ok(Value::Handle(handle))
        } else {
            Ok(value)
        }
    }

    pub(super) fn intrinsic_regex(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
    ) -> Result<Value, InterpretError> {
        let context = &mut *self.context as *mut Context;
        let handle = |index: usize| -> Result<*mut u8, InterpretError> {
            operands
                .get(index)
                .ok_or_else(|| {
                    self.invalid(None, format!("RegExp.{operation} has no operand {index}"))
                })?
                .as_handle()
        };
        let integer = |index: usize| -> Result<i32, InterpretError> {
            Ok(operands
                .get(index)
                .ok_or_else(|| {
                    self.invalid(None, format!("RegExp.{operation} has no operand {index}"))
                })?
                .as_i64()? as i32)
        };
        let value = match operation {
            // SAFETY: operands are live runtime strings.
            "New" => Value::Handle(unsafe {
                ffi::subscript_rt_regex_new(context, handle(0)?, handle(1)?, 0)
            }),
            // SAFETY: operands are a live regex and string.
            "Test" => Value::Bool(
                unsafe { ffi::subscript_rt_regex_test(context, handle(0)?, handle(1)?, 0) } != 0,
            ),
            // SAFETY: operand is a live regex.
            "Source" => {
                Value::Handle(unsafe { ffi::subscript_rt_regex_source(context, handle(0)?, 0) })
            }
            // SAFETY: operand is a live regex.
            "Global" => {
                Value::Bool(unsafe { ffi::subscript_rt_regex_global(context, handle(0)?, 0) } != 0)
            }
            // SAFETY: operand is a live regex.
            "IgnoreCase" => Value::Bool(
                unsafe { ffi::subscript_rt_regex_ignore_case(context, handle(0)?, 0) } != 0,
            ),
            // SAFETY: operand is a live regex.
            "Multiline" => Value::Bool(
                unsafe { ffi::subscript_rt_regex_multiline(context, handle(0)?, 0) } != 0,
            ),
            // SAFETY: operand is a live regex.
            "DotAll" => {
                Value::Bool(unsafe { ffi::subscript_rt_regex_dot_all(context, handle(0)?, 0) } != 0)
            }
            // SAFETY: operand is a live regex.
            "Unicode" => {
                Value::Bool(unsafe { ffi::subscript_rt_regex_unicode(context, handle(0)?, 0) } != 0)
            }
            // SAFETY: operand is a live regex.
            "HasIndices" => Value::Bool(
                unsafe { ffi::subscript_rt_regex_has_indices(context, handle(0)?, 0) } != 0,
            ),
            // SAFETY: operand is a live regex.
            "Sticky" => {
                Value::Bool(unsafe { ffi::subscript_rt_regex_sticky(context, handle(0)?, 0) } != 0)
            }
            // SAFETY: operand is a live regex.
            "ToString" => {
                Value::Handle(unsafe { ffi::subscript_rt_regex_to_string(context, handle(0)?, 0) })
            }
            // SAFETY: operand is a live regex.
            "Flags" => {
                Value::Handle(unsafe { ffi::subscript_rt_regex_flags(context, handle(0)?, 0) })
            }
            // SAFETY: operands are a live subject string and regex.
            "Search" => Value::I(unsafe {
                ffi::subscript_rt_regex_search(context, handle(0)?, handle(1)?, 0)
            } as i64),
            // SAFETY: operands are live subject, regex, and replacement handles.
            "Replace" => Value::Handle(unsafe {
                ffi::subscript_rt_regex_replace(context, handle(0)?, handle(1)?, handle(2)?, 0)
            }),
            // SAFETY: operands are live subject, regex, and replacement handles.
            "ReplaceAll" => Value::Handle(unsafe {
                ffi::subscript_rt_regex_replace_all(context, handle(0)?, handle(1)?, handle(2)?, 0)
            }),
            // SAFETY: operands are a live subject string and regex.
            "Split" => Value::Handle(unsafe {
                ffi::subscript_rt_regex_split(context, handle(0)?, handle(1)?, integer(2)?, 0)
            }),
            // SAFETY: operand is a live regex and the group is an integer.
            "MatchStart" => Value::I(unsafe {
                ffi::subscript_rt_regex_match_start(context, handle(0)?, integer(1)?, 0)
            } as i64),
            // SAFETY: operand is a live regex and the group is an integer.
            "MatchEnd" => Value::I(unsafe {
                ffi::subscript_rt_regex_match_end(context, handle(0)?, integer(1)?, 0)
            } as i64),
            _ => return Err(self.invalid(None, format!("unknown RegExp intrinsic {operation}"))),
        };
        self.check_runtime(&Pos::new("<regexp>", 1, 1))?;
        if let Value::Handle(handle) = value {
            Ok(Value::Handle(handle))
        } else {
            Ok(value)
        }
    }

    pub(super) fn intrinsic_json(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
        result_ty: Option<&l::ValueType>,
    ) -> Result<Value, InterpretError> {
        let context = &mut *self.context as *mut Context;
        let operand = |index: usize| -> Result<&Value, InterpretError> {
            operands.get(index).ok_or_else(|| {
                self.invalid(None, format!("JSON.{operation} has no operand {index}"))
            })
        };
        let id = |index: usize| -> Result<u64, InterpretError> { operand(index)?.as_u64() };
        let integer = |index: usize| -> Result<i64, InterpretError> { operand(index)?.as_i64() };
        let handle =
            |index: usize| -> Result<*mut u8, InterpretError> { operand(index)?.as_handle() };
        let value = match operation {
            // SAFETY: the Context owns the transient builder table.
            "Begin" => Value::U(unsafe { ffi::subscript_rt_json_begin(context, 0) }),
            // SAFETY: the Context owns the transient builder table.
            "BeginTracked" => Value::U(unsafe { ffi::subscript_rt_json_begin_tracked(context, 0) }),
            // SAFETY: builder was obtained from this Context.
            "Finish" => Value::Handle(unsafe { ffi::subscript_rt_json_finish(context, id(0)?, 0) }),
            // SAFETY: builder and string operands are live.
            "Raw" => {
                unsafe { ffi::subscript_rt_json_raw(context, id(0)?, handle(1)?, 0) };
                Value::Void
            }
            // SAFETY: builder and string operands are live.
            "Str" => {
                unsafe { ffi::subscript_rt_json_str(context, id(0)?, handle(1)?, 0) };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "I32" => {
                unsafe { ffi::subscript_rt_json_i32(context, id(0)?, integer(1)? as i32, 0) };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "U32" => {
                unsafe {
                    ffi::subscript_rt_json_u32(context, id(0)?, operand(1)?.as_u64()? as u32, 0)
                };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "I64" => {
                unsafe { ffi::subscript_rt_json_i64(context, id(0)?, integer(1)?, 0) };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "U64" => {
                unsafe { ffi::subscript_rt_json_u64(context, id(0)?, operand(1)?.as_u64()?, 0) };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "F32" => {
                unsafe {
                    ffi::subscript_rt_json_f32(context, id(0)?, operand(1)?.as_f64()? as f32, 0)
                };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "F64" => {
                unsafe { ffi::subscript_rt_json_f64(context, id(0)?, operand(1)?.as_f64()?, 0) };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "Bool" => {
                unsafe {
                    ffi::subscript_rt_json_bool(
                        context,
                        id(0)?,
                        u8::from(operand(1)?.as_bool()?),
                        0,
                    )
                };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "Date" => {
                unsafe { ffi::subscript_rt_json_date(context, id(0)?, integer(1)?, 0) };
                Value::Void
            }
            // SAFETY: builder was obtained from this Context.
            "Null" => {
                unsafe { ffi::subscript_rt_json_null(context, id(0)?, 0) };
                Value::Void
            }
            // SAFETY: builder and reference are live.
            "Visit" => Value::Bool(
                unsafe { ffi::subscript_rt_json_visit(context, id(0)?, handle(1)?, 0) } != 0,
            ),
            // SAFETY: builder and reference are live.
            "Leave" => {
                unsafe { ffi::subscript_rt_json_leave(context, id(0)?, handle(1)?, 0) };
                Value::Void
            }
            // SAFETY: input is a live string.
            "ParseBegin" => {
                Value::U(unsafe { ffi::subscript_rt_json_parse_begin(context, handle(0)?, 0) })
            }
            // SAFETY: parser was obtained from this Context.
            "ParseEnd" => {
                unsafe { ffi::subscript_rt_json_parse_end(context, id(0)?, 0) };
                Value::Void
            }
            // SAFETY: parser was obtained from this Context.
            "ParseRoot" => {
                Value::U(unsafe { ffi::subscript_rt_json_parse_root(context, id(0)?, 0) })
            }
            // SAFETY: parser/node handles and discriminator are LIR integers.
            "ParseIsKind" => Value::Bool(
                unsafe {
                    ffi::subscript_rt_json_parse_is_kind(context, id(0)?, id(1)?, id(2)? as u32, 0)
                } != 0,
            ),
            // SAFETY: parser/node handles and discriminator are LIR integers.
            "ParseNumberFits" => Value::Bool(
                unsafe {
                    ffi::subscript_rt_json_parse_number_fits(
                        context,
                        id(0)?,
                        id(1)?,
                        id(2)? as u32,
                        0,
                    )
                } != 0,
            ),
            // SAFETY: parser/node handles are live.
            "ParseNumber" => {
                let number =
                    unsafe { ffi::subscript_rt_json_parse_number(context, id(0)?, id(1)?, 0) };
                if matches!(result_ty, Some(l::ValueType::Data(Type::F32))) {
                    Value::F32(number as f32)
                } else {
                    Value::F64(number)
                }
            }
            // SAFETY: parser/node handles and discriminator are LIR integers.
            "ParseInteger" => {
                let bits = unsafe {
                    ffi::subscript_rt_json_parse_integer(context, id(0)?, id(1)?, id(2)? as u32, 0)
                };
                match result_ty {
                    Some(l::ValueType::Data(ty)) => self.integer_result(ty, bits)?,
                    _ => Value::U(bits),
                }
            }
            // SAFETY: parser/node handles are live.
            "ParseBool" => Value::Bool(
                unsafe { ffi::subscript_rt_json_parse_bool(context, id(0)?, id(1)?, 0) } != 0,
            ),
            // SAFETY: parser/node handles are live.
            "ParseString" => Value::Handle(unsafe {
                ffi::subscript_rt_json_parse_string(context, id(0)?, id(1)?, 0)
            }),
            // SAFETY: parser/node handles are live.
            "ParseArrayLen" => Value::I(unsafe {
                ffi::subscript_rt_json_parse_array_len(context, id(0)?, id(1)?, 0)
            } as i64),
            // SAFETY: parser/node handles are live and index is an integer.
            "ParseArrayGet" => Value::U(unsafe {
                ffi::subscript_rt_json_parse_array_get(
                    context,
                    id(0)?,
                    id(1)?,
                    integer(2)? as i32,
                    0,
                )
            }),
            // SAFETY: parser/node handles and key string are live.
            "ParseObjectGet" => Value::U(unsafe {
                ffi::subscript_rt_json_parse_object_get(context, id(0)?, id(1)?, handle(2)?, 0)
            }),
            // SAFETY: the Context is live.
            "ParseFailure" => {
                Value::Handle(unsafe { ffi::subscript_rt_json_parse_failure(context, 0) })
            }
            _ => return Err(self.invalid(None, format!("unknown JSON intrinsic {operation}"))),
        };
        self.check_runtime(&Pos::new("<json>", 1, 1))?;
        if let Value::Handle(handle) = value {
            Ok(Value::Handle(handle))
        } else {
            Ok(value)
        }
    }
}
