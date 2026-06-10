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
//!   1. DISPATCH — `OPERATOR_ARITY` derives an operator's operand shape from its
//!      `tx` superclass: UnaryNumericOp -> unary `value`; BinaryArithmetic /
//!      BinaryComparison -> binary; BooleanLogic -> n-ary `operands`; plus the
//!      two structural shapes the hierarchy can't imply (`Not`'s `operand`,
//!      `Clip`'s ternary).
//!   2. PRIMITIVES — the authored, determinism-bearing folds (`UNARY` / `BINARY`).
//!   3. THE FOLD — [`evaluate`]: operators recurse + apply a primitive; literals
//!      yield their numeric value; EVERYTHING ELSE (a typed VarRef, a domain
//!      `Step`/`Time`/`Smooth`, any future leaf) is handed to the caller's
//!      `resolve`. The runtime never privileges `tx.VarRef` — it is just one leaf
//!      a domain may resolve.
//!
//! Value domain: uniform `f64`. Booleans and comparison results are `1.0`/`0.0`.

use serde_json::Value;

/// The frozen expression-runtime version (determinism contract). Not hashed.
pub const EXPRESSION_RUNTIME_VERSION: &str = "1";

const TX: &str = "kanonak.org/transformations";
const MATH: &str = "kanonak.org/math";

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
fn operator_arity(typ: &str) -> Option<Arity> {
    // BinaryArithmetic -> arithLeft/arithRight.
    if typ == format!("{TX}/Add")
        || typ == format!("{TX}/Subtract")
        || typ == format!("{TX}/Multiply")
        || typ == format!("{TX}/Divide")
        || typ == format!("{MATH}/Power")
        || typ == format!("{MATH}/Modulo")
        || typ == format!("{MATH}/Minimum")
        || typ == format!("{MATH}/Maximum")
    {
        return Some(ARITH);
    }
    // UnaryNumericOp -> value.
    if typ == format!("{TX}/Abs")
        || typ == format!("{TX}/Negate")
        || typ == format!("{MATH}/Exp")
        || typ == format!("{MATH}/Ln")
        || typ == format!("{MATH}/Log10")
        || typ == format!("{MATH}/Sqrt")
        || typ == format!("{MATH}/Floor")
        || typ == format!("{MATH}/Ceil")
        || typ == format!("{MATH}/Round")
        || typ == format!("{MATH}/Sign")
    {
        return Some(VALUE);
    }
    // BinaryComparison -> compareLeft/compareRight.
    if typ == format!("{TX}/Equals")
        || typ == format!("{TX}/GreaterThan")
        || typ == format!("{TX}/LessThan")
        || typ == format!("{TX}/GreaterThanOrEqual")
        || typ == format!("{TX}/LessThanOrEqual")
    {
        return Some(COMPARE);
    }
    // BooleanLogic -> operands list.
    if typ == format!("{TX}/And") || typ == format!("{TX}/Or") {
        return Some(Arity::Nary { operands: "operands" });
    }
    // Clip ternary.
    if typ == format!("{MATH}/Clip") {
        return Some(Arity::Ternary { a: "clipValue", b: "clipLower", c: "clipUpper" });
    }
    // `Not` is a direct Expression subclass with boolean (not numeric-unary)
    // semantics — handled explicitly in `evaluate`, not via the tables.
    None
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
    if typ == format!("{TX}/Abs") {
        Ok(x.abs())
    } else if typ == format!("{TX}/Negate") {
        Ok(-x)
    } else if typ == format!("{MATH}/Exp") {
        Ok(x.exp())
    } else if typ == format!("{MATH}/Ln") {
        if x > 0.0 {
            Ok(x.ln())
        } else {
            err("Ln of a non-positive number")
        }
    } else if typ == format!("{MATH}/Log10") {
        if x > 0.0 {
            Ok(x.log10())
        } else {
            err("Log10 of a non-positive number")
        }
    } else if typ == format!("{MATH}/Sqrt") {
        if x >= 0.0 {
            Ok(x.sqrt())
        } else {
            err("Sqrt of a negative number")
        }
    } else if typ == format!("{MATH}/Floor") {
        Ok(x.floor())
    } else if typ == format!("{MATH}/Ceil") {
        Ok(x.ceil())
    } else if typ == format!("{MATH}/Round") {
        Ok(round_half_away(x))
    } else if typ == format!("{MATH}/Sign") {
        Ok(sign(x))
    } else {
        err(format!("{typ} has no unary primitive"))
    }
}

/// Binary primitive fold for `typ`, applied to `(a, b)`.
fn binary(typ: &str, a: f64, b: f64) -> Result<f64, ExpressionError> {
    if typ == format!("{TX}/Add") {
        Ok(a + b)
    } else if typ == format!("{TX}/Subtract") {
        Ok(a - b)
    } else if typ == format!("{TX}/Multiply") {
        Ok(a * b)
    } else if typ == format!("{TX}/Divide") {
        if b == 0.0 {
            err("Divide by zero")
        } else {
            Ok(a / b)
        }
    } else if typ == format!("{MATH}/Power") {
        Ok(a.powf(b))
    } else if typ == format!("{MATH}/Modulo") {
        floored_mod(a, b)
    } else if typ == format!("{MATH}/Minimum") {
        Ok(a.min(b))
    } else if typ == format!("{MATH}/Maximum") {
        Ok(a.max(b))
    } else if typ == format!("{TX}/Equals") {
        Ok(boolnum(a == b))
    } else if typ == format!("{TX}/GreaterThan") {
        Ok(boolnum(a > b))
    } else if typ == format!("{TX}/LessThan") {
        Ok(boolnum(a < b))
    } else if typ == format!("{TX}/GreaterThanOrEqual") {
        Ok(boolnum(a >= b))
    } else if typ == format!("{TX}/LessThanOrEqual") {
        Ok(boolnum(a <= b))
    } else {
        err(format!("{typ} has no binary primitive"))
    }
}

/// Numeric value of a literal node, or `None` if it is not a literal.
fn literal_value(node: &Value, typ: &str) -> Option<f64> {
    if typ == format!("{TX}/IntegerLiteral") {
        node.get("integerLiteral").and_then(as_number)
    } else if typ == format!("{TX}/DecimalLiteral") {
        node.get("decimalLiteral").and_then(as_number)
    } else if typ == format!("{TX}/BooleanLiteral") {
        let v = node.get("booleanLiteral");
        let truthy = matches!(v, Some(Value::Bool(true)))
            || matches!(v, Some(Value::String(s)) if s == "true");
        Some(boolnum(truthy))
    } else {
        None
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

fn node_type(node: &Value) -> Result<String, ExpressionError> {
    match node.get("type").and_then(|t| t.as_str()) {
        Some(t) => Ok(t.to_string()),
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

        if let Some(arity) = operator_arity(&typ) {
            return match arity {
                Arity::Unary { operand: key } => {
                    let x = go(operand(node, &typ, key)?, ctx, resolve)?;
                    unary(&typ, x)
                }
                Arity::Binary { left, right } => {
                    let a = go(operand(node, &typ, left)?, ctx, resolve)?;
                    let b = go(operand(node, &typ, right)?, ctx, resolve)?;
                    binary(&typ, a, b)
                }
                Arity::Nary { operands } => {
                    let items = match node.get(operands).and_then(|v| v.as_array()) {
                        Some(arr) => arr,
                        None => return err(format!("{typ} expects an '{operands}' list")),
                    };
                    let is_and = typ == format!("{TX}/And");
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
                    let v = go(operand(node, &typ, a)?, ctx, resolve)?;
                    let lo = go(operand(node, &typ, b)?, ctx, resolve)?;
                    let hi = go(operand(node, &typ, c)?, ctx, resolve)?;
                    Ok(v.max(lo).min(hi))
                }
            };
        }

        if typ == format!("{TX}/Not") {
            let inner = go(operand(node, &typ, "operand")?, ctx, resolve)?;
            return Ok(boolnum(!truthy(inner)));
        }

        if let Some(lit) = literal_value(node, &typ) {
            return Ok(lit);
        }

        // Not an operator or literal — a binding or domain leaf. The caller owns it.
        let mut recurse =
            |n: &Value, c: &mut C| -> Result<f64, ExpressionError> { go(n, c, resolve) };
        resolve(node, ctx, &mut recurse)
    }

    go(node, ctx, resolve)
}
