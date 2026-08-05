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

/// Resolve an identity leaf inside an ordered comparison — any operand node
/// that is not a `tx.UriLiteral` — to a member's canonical versionless URI
/// (`publisher/package/name`). The identity-domain mirror of [`Resolve`]: the
/// kernel owns the constant leaf, the caller owns bindings.
pub type ResolveRef<'a, C> = &'a dyn Fn(&Value, &mut C) -> Result<String, ExpressionError>;

/// The transitive closures ordered comparisons consult, keyed by the ordering
/// property's canonical URI, then by member: `closures[property][from]` is the
/// set of members `from` reaches. Flat, already-closed data — typically the SDK
/// reasoner's `prp-trp` saturation emitted at code-generation time. The kernel
/// does set membership only; it never computes a closure, resolves a package,
/// or reasons.
pub type ClosureTable =
    std::collections::HashMap<String, std::collections::HashMap<String, Vec<String>>>;

/// Optional evaluation context for the ordered comparisons (`IsAtLeast`,
/// `Dominates`). Absent (or missing a needed entry), an ordered comparison
/// fails loudly — never a silent false from a missing table.
pub struct EvalOptions<'a, C> {
    pub closures: Option<&'a ClosureTable>,
    pub resolve_ref: Option<ResolveRef<'a, C>>,
}

/// One node of an evaluation trace — the verdict tree [`explain`] returns.
/// Mirrors the expression: `typ` is the node's type URI, `value` its result
/// (`1.0`/`0.0` for booleans), `children` the operand traces in evaluation
/// order. A short-circuited operand is simply ABSENT from `children` — the
/// trace is truthful about what ran. Ordered comparisons carry their resolved
/// operand identities as `left_ref`/`right_ref` instead of children. This is a
/// runtime return shape, not an ontology class.
#[derive(Debug, Clone)]
pub struct TraceNode {
    pub typ: String,
    pub value: f64,
    pub children: Vec<TraceNode>,
    pub left_ref: Option<String>,
    pub right_ref: Option<String>,
}

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

/// The identity an ordered comparison compares — a member's canonical
/// versionless URI. `tx.UriLiteral` is the kernel-known constant leaf (its
/// `refTo` IS the identity, the way a literal's value is its number); every
/// other node is the caller's, through `resolve_ref`.
fn identity_of<C>(
    node: &Value,
    ctx: &mut C,
    options: Option<&EvalOptions<C>>,
) -> Result<String, ExpressionError> {
    let typ = node_type(node)?;
    if typ == "kanonak.org/transformations/UriLiteral" {
        return match node.get("refTo").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(s.to_string()),
            _ => err("UriLiteral is missing refTo"),
        };
    }
    match options.and_then(|o| o.resolve_ref) {
        Some(resolve_ref) => resolve_ref(node, ctx),
        None => err(format!("No resolveRef supplied for identity leaf '{typ}'")),
    }
}

/// Fold an ordered comparison (`IsAtLeast` / `Dominates`) to `1.0`/`0.0` plus
/// the resolved operand identities. The ordering is the supplied closure for
/// the node's `viaProperty` — membership in already-closed data, nothing more.
/// Identity is canonical versionless URI string equality, matching
/// `tx.Equals`' identity rule. `IsAtLeast` folds reflexivity into the operator
/// (same member → 1); `Dominates` is strict (same member → 0). Two members
/// with no path yield 0 — fail-closed — but a MISSING closure table is a
/// configuration failure and errors loudly.
fn fold_ordered<C>(
    node: &Value,
    typ: &str,
    ctx: &mut C,
    options: Option<&EvalOptions<C>>,
) -> Result<(f64, String, String), ExpressionError> {
    let via = match node.get("viaProperty").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return err(format!("{typ} is missing viaProperty")),
    };
    let left = identity_of(operand(node, typ, "compareLeft")?, ctx, options)?;
    let right = identity_of(operand(node, typ, "compareRight")?, ctx, options)?;
    let closure = match options.and_then(|o| o.closures).and_then(|c| c.get(via)) {
        Some(c) => c,
        None => return err(format!("No closure supplied for ordering property '{via}'")),
    };
    let value = if left == right {
        boolnum(typ == "kanonak.org/transformations/IsAtLeast")
    } else {
        boolnum(closure.get(&left).map_or(false, |set| set.iter().any(|m| m == &right)))
    };
    Ok((value, left, right))
}

/// Evaluate an expression tree to a number. Operators fold via the frozen
/// dispatch + primitive tables; literals yield their numeric value; any other
/// node is delegated to `resolve`.
pub fn evaluate<C>(
    node: &Value,
    ctx: &mut C,
    resolve: Resolve<C>,
) -> Result<f64, ExpressionError> {
    evaluate_with_options(node, ctx, resolve, None)
}

/// [`evaluate`] with the ordered-comparison evaluation context (closures +
/// identity-leaf resolution). `options` is only consulted when an `IsAtLeast` /
/// `Dominates` node is reached; `None` is valid for trees without them.
pub fn evaluate_with_options<C>(
    node: &Value,
    ctx: &mut C,
    resolve: Resolve<C>,
    options: Option<&EvalOptions<C>>,
) -> Result<f64, ExpressionError> {
    fn go<C>(
        node: &Value,
        ctx: &mut C,
        resolve: Resolve<C>,
        options: Option<&EvalOptions<C>>,
    ) -> Result<f64, ExpressionError> {
        let typ = node_type(node)?;

        if let Some(arity) = operator_arity(typ) {
            return match arity {
                Arity::Unary { operand: key } => {
                    let x = go(operand(node, typ, key)?, ctx, resolve, options)?;
                    unary(typ, x)
                }
                Arity::Binary { left, right } => {
                    let a = go(operand(node, typ, left)?, ctx, resolve, options)?;
                    let b = go(operand(node, typ, right)?, ctx, resolve, options)?;
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
                        let v = truthy(go(item, ctx, resolve, options)?);
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
                    let v = go(operand(node, typ, a)?, ctx, resolve, options)?;
                    let lo = go(operand(node, typ, b)?, ctx, resolve, options)?;
                    let hi = go(operand(node, typ, c)?, ctx, resolve, options)?;
                    Ok(v.max(lo).min(hi))
                }
            };
        }

        if typ == "kanonak.org/transformations/Not" {
            let inner = go(operand(node, typ, "operand")?, ctx, resolve, options)?;
            return Ok(boolnum(!truthy(inner)));
        }

        if typ == "kanonak.org/transformations/IsAtLeast"
            || typ == "kanonak.org/transformations/Dominates"
        {
            return fold_ordered(node, typ, ctx, options).map(|(v, _, _)| v);
        }

        if let Some(lit) = literal_value(node, typ) {
            return Ok(lit);
        }

        // Not an operator or literal — a binding or domain leaf. The caller owns it.
        let mut recurse =
            |n: &Value, c: &mut C| -> Result<f64, ExpressionError> { go(n, c, resolve, options) };
        resolve(node, ctx, &mut recurse)
    }

    go(node, ctx, resolve, options)
}

/// Evaluate an expression tree and return the verdict tree — the regex-debugger
/// view: every evaluated node, its own result, and (for ordered comparisons)
/// the identities it compared. The root's `value` is exactly what [`evaluate`]
/// returns for the same inputs; the conformance suite runs every vector through
/// both and requires agreement, so the two entry points cannot drift. Kept
/// separate from `evaluate` so the hot path never pays for trace allocation.
/// Errors propagate exactly as in `evaluate` — a failed evaluation yields an
/// error, not a partial trace.
pub fn explain<C>(
    node: &Value,
    ctx: &mut C,
    resolve: Resolve<C>,
    options: Option<&EvalOptions<C>>,
) -> Result<TraceNode, ExpressionError> {
    fn leaf(typ: &str, value: f64) -> TraceNode {
        TraceNode { typ: typ.to_string(), value, children: Vec::new(), left_ref: None, right_ref: None }
    }
    fn parent(typ: &str, value: f64, children: Vec<TraceNode>) -> TraceNode {
        TraceNode { typ: typ.to_string(), value, children, left_ref: None, right_ref: None }
    }

    fn go<C>(
        node: &Value,
        ctx: &mut C,
        resolve: Resolve<C>,
        options: Option<&EvalOptions<C>>,
    ) -> Result<TraceNode, ExpressionError> {
        let typ = node_type(node)?;

        if let Some(arity) = operator_arity(typ) {
            return match arity {
                Arity::Unary { operand: key } => {
                    let x = go(operand(node, typ, key)?, ctx, resolve, options)?;
                    let value = unary(typ, x.value)?;
                    Ok(parent(typ, value, vec![x]))
                }
                Arity::Binary { left, right } => {
                    let a = go(operand(node, typ, left)?, ctx, resolve, options)?;
                    let b = go(operand(node, typ, right)?, ctx, resolve, options)?;
                    let value = binary(typ, a.value, b.value)?;
                    Ok(parent(typ, value, vec![a, b]))
                }
                Arity::Nary { operands } => {
                    let items = match node.get(operands).and_then(|v| v.as_array()) {
                        Some(arr) => arr,
                        None => return err(format!("{typ} expects an '{operands}' list")),
                    };
                    let is_and = typ == "kanonak.org/transformations/And";
                    let mut children = Vec::new();
                    for item in items {
                        let child = go(item, ctx, resolve, options)?;
                        let v = truthy(child.value);
                        children.push(child);
                        // Same short-circuit as `evaluate`: operands after the
                        // deciding one are never evaluated and never appear.
                        if is_and && !v {
                            return Ok(parent(typ, 0.0, children));
                        }
                        if !is_and && v {
                            return Ok(parent(typ, 1.0, children));
                        }
                    }
                    Ok(parent(typ, boolnum(is_and), children))
                }
                Arity::Ternary { a, b, c } => {
                    let v = go(operand(node, typ, a)?, ctx, resolve, options)?;
                    let lo = go(operand(node, typ, b)?, ctx, resolve, options)?;
                    let hi = go(operand(node, typ, c)?, ctx, resolve, options)?;
                    let value = v.value.max(lo.value).min(hi.value);
                    Ok(parent(typ, value, vec![v, lo, hi]))
                }
            };
        }

        if typ == "kanonak.org/transformations/Not" {
            let x = go(operand(node, typ, "operand")?, ctx, resolve, options)?;
            let value = boolnum(!truthy(x.value));
            return Ok(parent(typ, value, vec![x]));
        }

        if typ == "kanonak.org/transformations/IsAtLeast"
            || typ == "kanonak.org/transformations/Dominates"
        {
            let (value, left, right) = fold_ordered(node, typ, ctx, options)?;
            return Ok(TraceNode {
                typ: typ.to_string(),
                value,
                children: Vec::new(),
                left_ref: Some(left),
                right_ref: Some(right),
            });
        }

        if let Some(lit) = literal_value(node, typ) {
            return Ok(leaf(typ, lit));
        }

        // Numeric recursion for subtrees the caller's `resolve` re-enters: those
        // folds happen inside the caller and are invisible to the trace. Only
        // kernel-visited nodes appear.
        let mut recurse = |n: &Value, c: &mut C| -> Result<f64, ExpressionError> {
            evaluate_with_options(n, c, resolve, options)
        };
        let value = resolve(node, ctx, &mut recurse)?;
        Ok(leaf(typ, value))
    }

    go(node, ctx, resolve, options)
}
