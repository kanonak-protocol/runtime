//! Kanonak expression runtime (expressionRuntimeVersion "1").
//!
//! A small, deterministic tree-walker that folds a `kanonak.org/transformations`
//! (`tx`) + `kanonak.org/math` expression tree to a single number. A faithful
//! port of the reference TypeScript kernel, verified against the shared parity
//! vectors — including the determinism traps (Round half-away-from-zero, floored
//! Modulo, Sign(0)=0, comparisons as 1/0).
//!
//! Three layers, exactly as the reference establishes:
//!
//!   1. DISPATCH — `operator_arity` derives an operator's operand shape from its
//!      `tx` superclass: UnaryNumericOp -> unary `value`; BinaryArithmetic /
//!      BinaryComparison -> binary; BooleanLogic -> n-ary `operands`; plus the
//!      two structural shapes the hierarchy can't imply (`Not`'s `operand`,
//!      `Clip`'s ternary).
//!   2. PRIMITIVES — the authored, determinism-bearing folds (`unary` / `binary`).
//!   3. THE FOLD — [`evaluate`]: operators recurse + apply a primitive; literals
//!      yield their numeric value; EVERYTHING ELSE (a typed VarRef, a domain
//!      `Step`/`Time`/`Smooth`, any future leaf) is handed to the caller's
//!      `resolve`. The runtime never privileges `tx.VarRef` — it is just one leaf
//!      a domain may resolve.
//!
//! Value domain: uniform `f64`. Booleans and comparison results are `1.0`/`0.0`.
//!
//! Operator/literal type tags are matched against `&'static str` literals (the
//! frozen canonical URIs) — no allocation in the evaluation hot path, which
//! matters for the per-step integrators (e.g. RK4) that re-evaluate an equation
//! thousands of times.

use serde_json::Value;

/// The frozen expression-runtime version (determinism contract). Not hashed.
pub const EXPRESSION_RUNTIME_VERSION: &str = "1";

/// An evaluation error. Determinism traps (Divide/Modulo by zero, Ln/Log10 of
/// <=0, Sqrt of <0) and structural problems raise this — never `NaN`/`Inf`.
#[derive(Debug, Clone)]
pub struct ExpressionError(pub String);

impl std::fmt::Display for ExpressionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ExpressionError {}

fn err<T>(msg: impl Into<String>) -> Result<T, ExpressionError> {
    Err(ExpressionError(msg.into()))
}

/// Resolve any node the kernel does not recognise as an operator or literal — a
/// binding (`tx.VarRef`, a domain's typed `refersTo` VarRef) or a domain leaf
/// (`Step`, `Time`, `Smooth`…) — to a number. `ctx` is opaque caller state.
/// `recurse` is handed back so a domain leaf containing sub-expressions can
/// recurse into the kernel.
pub type Resolve<'a, C> =
    &'a dyn Fn(&Value, &mut C, &mut dyn FnMut(&Value, &mut C) -> Result<f64, ExpressionError>)
        -> Result<f64, ExpressionError>;

/// Operand shape per operator, derived from the `tx` superclass hierarchy.
enum Arity {
    Unary { operand: &'static str },
    Binary { left: &'static str, right: &'static str },
    Nary { operands: &'static str },
    Ternary { a: &'static str, b: &'static str, c: &'static str },
}

const ARITH: Arity = Arity::Binary { left: "arithLeft", right: "arithRight" };
const COMPARE: Arity = Arity::Binary { left: "compareLeft", right: "compareRight" };
const VALUE: Arity = Arity::Unary { operand: "value" };

/// The frozen dispatch table — maps each operator URI to its operand shape.
/// `Not` is a direct Expression subclass with boolean (not numeric-unary)
/// semantics, so it is handled explicitly in `evaluate`, not via this table.
fn operator_arity(typ: &str) -> Option<Arity> {
    match typ {
        // BinaryArithmetic -> arithLeft/arithRight.
        "kanonak.org/transformations/Add"
        | "kanonak.org/transformations/Subtract"
        | "kanonak.org/transformations/Multiply"
        | "kanonak.org/transformations/Divide"
        | "kanonak.org/math/Power"
        | "kanonak.org/math/Modulo"
        | "kanonak.org/math/Minimum"
        | "kanonak.org/math/Maximum" => Some(ARITH),
        // UnaryNumericOp -> value.
        "kanonak.org/transformations/Abs"
        | "kanonak.org/transformations/Negate"
        | "kanonak.org/math/Exp"
        | "kanonak.org/math/Ln"
        | "kanonak.org/math/Log10"
        | "kanonak.org/math/Sqrt"
        | "kanonak.org/math/Floor"
        | "kanonak.org/math/Ceil"
        | "kanonak.org/math/Round"
        | "kanonak.org/math/Sign" => Some(VALUE),
        // BinaryComparison -> compareLeft/compareRight.
        "kanonak.org/transformations/Equals"
        | "kanonak.org/transformations/GreaterThan"
        | "kanonak.org/transformations/LessThan"
        | "kanonak.org/transformations/GreaterThanOrEqual"
        | "kanonak.org/transformations/LessThanOrEqual" => Some(COMPARE),
        // BooleanLogic -> operands list.
        "kanonak.org/transformations/And" | "kanonak.org/transformations/Or" => {
            Some(Arity::Nary { operands: "operands" })
        }
        // Clip ternary.
        "kanonak.org/math/Clip" => {
            Some(Arity::Ternary { a: "clipValue", b: "clipLower", c: "clipUpper" })
        }
        _ => None,
    }
}

/// Floored modulo (the host `%` truncates toward zero): Modulo(-7,3) = 2.
fn floored_mod(a: f64, b: f64) -> Result<f64, ExpressionError> {
    if b == 0.0 {
        return err("Modulo by zero");
    }
    Ok(a - b * (a / b).floor())
}

/// Round half away from zero: Round(-2.5) = -3, Round(2.5) = 3.
fn round_half_away(a: f64) -> f64 {
    // sign(x) * floor(abs(x) + 0.5), avoiding any half-to-even native rounding.
    if a < 0.0 {
        -(((-a) + 0.5).floor())
    } else {
        (a + 0.5).floor()
    }
}

fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

fn truthy(n: f64) -> bool {
    n != 0.0
}

fn boolnum(b: bool) -> f64 {
    if b {
        1.0
    } else {
        0.0
    }
}

/// Unary primitive fold for `typ`, applied to `x`. The authored,
/// determinism-bearing table — matched per language.
fn unary(typ: &str, x: f64) -> Result<f64, ExpressionError> {
    match typ {
        "kanonak.org/transformations/Abs" => Ok(x.abs()),
        "kanonak.org/transformations/Negate" => Ok(-x),
        "kanonak.org/math/Exp" => Ok(x.exp()),
        "kanonak.org/math/Ln" => {
            if x > 0.0 {
                Ok(x.ln())
            } else {
                err("Ln of a non-positive number")
            }
        }
        "kanonak.org/math/Log10" => {
            if x > 0.0 {
                Ok(x.log10())
            } else {
                err("Log10 of a non-positive number")
            }
        }
        "kanonak.org/math/Sqrt" => {
            if x >= 0.0 {
                Ok(x.sqrt())
            } else {
                err("Sqrt of a negative number")
            }
        }
        "kanonak.org/math/Floor" => Ok(x.floor()),
        "kanonak.org/math/Ceil" => Ok(x.ceil()),
        "kanonak.org/math/Round" => Ok(round_half_away(x)),
        "kanonak.org/math/Sign" => Ok(sign(x)),
        _ => err(format!("{typ} has no unary primitive")),
    }
}

/// Binary primitive fold for `typ`, applied to `(a, b)`.
fn binary(typ: &str, a: f64, b: f64) -> Result<f64, ExpressionError> {
    match typ {
        "kanonak.org/transformations/Add" => Ok(a + b),
        "kanonak.org/transformations/Subtract" => Ok(a - b),
        "kanonak.org/transformations/Multiply" => Ok(a * b),
        "kanonak.org/transformations/Divide" => {
            if b == 0.0 {
                err("Divide by zero")
            } else {
                Ok(a / b)
            }
        }
        "kanonak.org/math/Power" => Ok(a.powf(b)),
        "kanonak.org/math/Modulo" => floored_mod(a, b),
        "kanonak.org/math/Minimum" => Ok(a.min(b)),
        "kanonak.org/math/Maximum" => Ok(a.max(b)),
        "kanonak.org/transformations/Equals" => Ok(boolnum(a == b)),
        "kanonak.org/transformations/GreaterThan" => Ok(boolnum(a > b)),
        "kanonak.org/transformations/LessThan" => Ok(boolnum(a < b)),
        "kanonak.org/transformations/GreaterThanOrEqual" => Ok(boolnum(a >= b)),
        "kanonak.org/transformations/LessThanOrEqual" => Ok(boolnum(a <= b)),
        _ => err(format!("{typ} has no binary primitive")),
    }
}

/// Numeric value of a literal node, or `None` if it is not a literal.
fn literal_value(node: &Value, typ: &str) -> Option<f64> {
    match typ {
        "kanonak.org/transformations/IntegerLiteral" => node.get("integerLiteral").and_then(as_number),
        "kanonak.org/transformations/DecimalLiteral" => node.get("decimalLiteral").and_then(as_number),
        "kanonak.org/transformations/BooleanLiteral" => {
            let v = node.get("booleanLiteral");
            let truthy = matches!(v, Some(Value::Bool(true)))
                || matches!(v, Some(Value::String(s)) if s == "true");
            Some(boolnum(truthy))
        }
        _ => None,
    }
}

fn as_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        Value::Bool(b) => Some(boolnum(*b)),
        _ => None,
    }
}

/// The node's `type` tag, borrowed (no allocation).
fn node_type(node: &Value) -> Result<&str, ExpressionError> {
    match node.get("type").and_then(|t| t.as_str()) {
        Some(t) => Ok(t),
        None => err("node is missing a 'type'"),
    }
}

fn operand<'a>(node: &'a Value, typ: &str, key: &str) -> Result<&'a Value, ExpressionError> {
    match node.get(key) {
        Some(v) if v.is_object() => Ok(v),
        _ => err(format!("{typ} is missing operand '{key}'")),
    }
}

/// Evaluate an expression tree to a number. Operators fold via the frozen
/// dispatch + primitive tables; literals yield their numeric value; any other
/// node is delegated to `resolve`.
pub fn evaluate<C>(
    node: &Value,
    ctx: &mut C,
    resolve: Resolve<C>,
) -> Result<f64, ExpressionError> {
    fn go<C>(
        node: &Value,
        ctx: &mut C,
        resolve: Resolve<C>,
    ) -> Result<f64, ExpressionError> {
        let typ = node_type(node)?;

        if let Some(arity) = operator_arity(typ) {
            return match arity {
                Arity::Unary { operand: key } => {
                    let x = go(operand(node, typ, key)?, ctx, resolve)?;
                    unary(typ, x)
                }
                Arity::Binary { left, right } => {
                    let a = go(operand(node, typ, left)?, ctx, resolve)?;
                    let b = go(operand(node, typ, right)?, ctx, resolve)?;
                    binary(typ, a, b)
                }
                Arity::Nary { operands } => {
                    let items = match node.get(operands).and_then(|v| v.as_array()) {
                        Some(arr) => arr,
                        None => return err(format!("{typ} expects an '{operands}' list")),
                    };
                    let is_and = typ == "kanonak.org/transformations/And";
                    // Short-circuit; empty And vacuously true, empty Or vacuously false.
                    for item in items {
                        let v = truthy(go(item, ctx, resolve)?);
                        if is_and && !v {
                            return Ok(0.0);
                        }
                        if !is_and && v {
                            return Ok(1.0);
                        }
                    }
                    Ok(boolnum(is_and))
                }
                Arity::Ternary { a, b, c } => {
                    // Only Clip today: clamp clipValue into [clipLower, clipUpper].
                    let v = go(operand(node, typ, a)?, ctx, resolve)?;
                    let lo = go(operand(node, typ, b)?, ctx, resolve)?;
                    let hi = go(operand(node, typ, c)?, ctx, resolve)?;
                    Ok(v.max(lo).min(hi))
                }
            };
        }

        if typ == "kanonak.org/transformations/Not" {
            let inner = go(operand(node, typ, "operand")?, ctx, resolve)?;
            return Ok(boolnum(!truthy(inner)));
        }

        if let Some(lit) = literal_value(node, typ) {
            return Ok(lit);
        }

        // Not an operator or literal — a binding or domain leaf. The caller owns it.
        let mut recurse =
            |n: &Value, c: &mut C| -> Result<f64, ExpressionError> { go(n, c, resolve) };
        resolve(node, ctx, &mut recurse)
    }

    go(node, ctx, resolve)
}
