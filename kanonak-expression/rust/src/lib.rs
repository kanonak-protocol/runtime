//! Kanonak expression runtime (expressionRuntimeVersion "2").
//!
//! A small, deterministic tree-walker that folds a `kanonak.org/transformations`
//! (`tx`) + `kanonak.org/math` expression tree to a VALUE. A faithful port of
//! the reference TypeScript kernel, verified against the shared parity vectors
//! — including the determinism traps (Round half-away-from-zero, floored
//! Modulo, Sign(0)=0, comparisons as 1/0, the pinned Matches dialect).
//!
//! VALUE DOMAIN (v2): `number | string | ref | list` ([`EvalValue`]).
//! Booleans and comparison results remain `1.0`/`0.0` numbers; a ref is a
//! canonical versionless URI identity; ABSENCE IS THE EMPTY LIST — there is
//! no null. The caller's `resolve` may return any value: a property read is
//! just a caller leaf that returns a list. The kernel NEVER touches a graph.
//!
//! ERROR CONTRACT: computations fail LOUD (arithmetic on a non-number, an
//! aggregate over non-numeric elements, Min/Max/Average on empty, a nested
//! list in Join, an out-of-subset Matches pattern); predicates fail CLOSED
//! (`Equals` cross-kind and the ordering comparisons on non-numbers yield
//! `0.0`, never an error).
//!
//! LAMBDA BINDING: the iterating operators (Filter/ListMap/ForEach) bind
//! their `loopVar` per element; within their bodies — and only there — a
//! `tx.VarRef` naming a lexically-enclosing loopVar is resolved by the
//! kernel (innermost binder wins). Every other leaf is the caller's, and
//! recursion re-entered from inside `resolve` carries no frames.
//!
//! MATCHES: the pinned RE2-compatible XSD-regex subset. `.` and quantifiers
//! count Unicode CODE POINTS (this engine's native rune model); the shorthand
//! classes are ASCII by textual expansion; `\b`/`\B` compile as `(?-u:\b)` —
//! the ASCII word boundary. Out-of-subset constructs are loud errors.
//!
//! Operator/literal type tags are matched against `&'static str` literals (the
//! frozen canonical URIs) — no allocation in the evaluation hot path.

use serde_json::Value as Json;

/// The frozen expression-runtime version (determinism contract). Not hashed.
pub const EXPRESSION_RUNTIME_VERSION: &str = "2";

/// An evaluation error. Determinism traps and structural problems raise this —
/// never `NaN`/`Inf`, never a silent coercion.
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

/// The v2 value domain: `number | string | ref | list`. Booleans are
/// `1.0`/`0.0` numbers; absence is the empty list; there is no null. A `Ref`
/// carries a canonical versionless URI (`publisher/package/name`) and is
/// distinct from `Str` so `Equals` can hold the cross-kind-is-false rule.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalValue {
    Num(f64),
    Str(String),
    Ref(String),
    List(Vec<EvalValue>),
}

impl EvalValue {
    fn kind(&self) -> &'static str {
        match self {
            EvalValue::Num(_) => "number",
            EvalValue::Str(_) => "string",
            EvalValue::Ref(_) => "ref",
            EvalValue::List(_) => "list",
        }
    }
}

/// Resolve any node the kernel does not recognise as an operator or literal —
/// a binding (`tx.VarRef`), a host graph read (a property-read leaf returning
/// a list), or a domain leaf — to a value. `ctx` is opaque caller state;
/// `recurse` is handed back so a domain leaf containing sub-expressions can
/// recurse into the kernel (WITHOUT lambda frames — the caller's subtrees are
/// the caller's scope).
pub type Resolve<'a, C> = &'a dyn Fn(
    &Json,
    &mut C,
    &mut dyn FnMut(&Json, &mut C) -> Result<EvalValue, ExpressionError>,
) -> Result<EvalValue, ExpressionError>;

/// Resolve an identity leaf inside an ordered comparison — any operand node
/// that is not a `tx.UriLiteral` — to a member's canonical versionless URI.
/// The identity-domain mirror of [`Resolve`].
pub type ResolveRef<'a, C> = &'a dyn Fn(&Json, &mut C) -> Result<String, ExpressionError>;

/// The transitive closures ordered comparisons consult, keyed by the ordering
/// property's canonical URI, then by member. Flat, already-closed data. The
/// kernel does set membership only; it never computes a closure or reasons.
pub type ClosureTable =
    std::collections::HashMap<String, std::collections::HashMap<String, Vec<String>>>;

/// Optional evaluation context for the ordered comparisons. Absent (or missing
/// a needed entry), an ordered comparison fails loudly.
pub struct EvalOptions<'a, C> {
    pub closures: Option<&'a ClosureTable>,
    pub resolve_ref: Option<ResolveRef<'a, C>>,
}

/// One node of an evaluation trace — the verdict tree [`explain`] returns.
/// Mirrors the expression; a short-circuited operand is ABSENT from
/// `children`; an iterating operator's children are its source trace followed
/// by one body trace per visited element; ordered comparisons carry
/// `left_ref`/`right_ref` instead of children. A runtime return shape, not an
/// ontology class.
#[derive(Debug, Clone)]
pub struct TraceNode {
    pub typ: String,
    pub value: EvalValue,
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

/// The frozen dispatch table for the numeric/boolean core. `Not` is handled
/// explicitly (boolean, not numeric-unary semantics).
fn operator_arity(typ: &str) -> Option<Arity> {
    match typ {
        "kanonak.org/transformations/Add"
        | "kanonak.org/transformations/Subtract"
        | "kanonak.org/transformations/Multiply"
        | "kanonak.org/transformations/Divide"
        | "kanonak.org/math/Power"
        | "kanonak.org/math/Modulo"
        | "kanonak.org/math/Minimum"
        | "kanonak.org/math/Maximum" => Some(ARITH),

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

        "kanonak.org/transformations/Equals"
        | "kanonak.org/transformations/GreaterThan"
        | "kanonak.org/transformations/LessThan"
        | "kanonak.org/transformations/GreaterThanOrEqual"
        | "kanonak.org/transformations/LessThanOrEqual" => Some(COMPARE),

        "kanonak.org/transformations/And" | "kanonak.org/transformations/Or" => {
            Some(Arity::Nary { operands: "operands" })
        }

        "kanonak.org/math/Clip" => Some(Arity::Ternary {
            a: "clipValue",
            b: "clipLower",
            c: "clipUpper",
        }),

        _ => None,
    }
}

/// The iterating operators' body operand, keyed by type URI.
fn iterator_body(typ: &str) -> Option<&'static str> {
    match typ {
        "kanonak.org/transformations/ForEach" => Some("emit"),
        "kanonak.org/transformations/ListMap" => Some("mapBody"),
        "kanonak.org/transformations/Filter" => Some("predicate"),
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
        -((-a + 0.5).floor())
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
        0.0 // matches JS Math.sign for +0/-0 within the always-finite domain
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

fn require_num(v: &EvalValue, op: &str) -> Result<f64, ExpressionError> {
    match v {
        EvalValue::Num(n) => Ok(*n),
        other => err(format!("{op} requires a numeric operand, got {}", other.kind())),
    }
}

/// Unary numeric primitives — the authored, determinism-bearing folds.
fn unary(typ: &str, x: f64) -> Result<f64, ExpressionError> {
    match typ {
        "kanonak.org/transformations/Abs" => Ok(x.abs()),
        "kanonak.org/transformations/Negate" => Ok(-x),
        "kanonak.org/math/Exp" => Ok(x.exp()),
        "kanonak.org/math/Ln" => {
            if x <= 0.0 {
                err("Ln of a non-positive number")
            } else {
                Ok(x.ln())
            }
        }
        "kanonak.org/math/Log10" => {
            if x <= 0.0 {
                err("Log10 of a non-positive number")
            } else {
                Ok(x.log10())
            }
        }
        "kanonak.org/math/Sqrt" => {
            if x < 0.0 {
                err("Sqrt of a negative number")
            } else {
                Ok(x.sqrt())
            }
        }
        "kanonak.org/math/Floor" => Ok(x.floor()),
        "kanonak.org/math/Ceil" => Ok(x.ceil()),
        "kanonak.org/math/Round" => Ok(round_half_away(x)),
        "kanonak.org/math/Sign" => Ok(sign(x)),
        _ => err(format!("No unary primitive for {typ}")),
    }
}

/// Binary ARITHMETIC primitives — computations, fail loud.
fn binary_arith(typ: &str, a: f64, b: f64) -> Result<f64, ExpressionError> {
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
        _ => err(format!("No arithmetic primitive for {typ}")),
    }
}

/// Ordering comparisons — PREDICATES: on non-numeric operands they fail
/// CLOSED (`0.0`), matching the reference engine and SHACL's verdict for a
/// non-comparable value.
fn binary_order(typ: &str, a: f64, b: f64) -> Option<f64> {
    match typ {
        "kanonak.org/transformations/GreaterThan" => Some(boolnum(a > b)),
        "kanonak.org/transformations/LessThan" => Some(boolnum(a < b)),
        "kanonak.org/transformations/GreaterThanOrEqual" => Some(boolnum(a >= b)),
        "kanonak.org/transformations/LessThanOrEqual" => Some(boolnum(a <= b)),
        _ => None,
    }
}

fn is_order_comparison(typ: &str) -> bool {
    matches!(
        typ,
        "kanonak.org/transformations/GreaterThan"
            | "kanonak.org/transformations/LessThan"
            | "kanonak.org/transformations/GreaterThanOrEqual"
            | "kanonak.org/transformations/LessThanOrEqual"
    )
}

/// Polymorphic value equality — the full `tx.Equals` contract: value equality
/// for scalars, URI-key identity for refs, lists never equal, cross-kind
/// false. Never errors.
fn values_equal(a: &EvalValue, b: &EvalValue) -> bool {
    match (a, b) {
        (EvalValue::Num(x), EvalValue::Num(y)) => x == y,
        (EvalValue::Str(x), EvalValue::Str(y)) => x == y,
        (EvalValue::Ref(x), EvalValue::Ref(y)) => x == y,
        _ => false,
    }
}

fn node_type(node: &Json) -> Result<&str, ExpressionError> {
    match node.get("type").and_then(Json::as_str) {
        Some(t) => Ok(t),
        None => err("Expression node has no 'type'"),
    }
}

fn operand<'a>(node: &'a Json, typ: &str, key: &str) -> Result<&'a Json, ExpressionError> {
    match node.get(key) {
        Some(v) if v.is_object() => Ok(v),
        _ => err(format!("{typ} is missing operand '{key}'")),
    }
}

/// Value of a literal node, or `None` if it is not a literal. `StringLiteral`
/// and `UriLiteral` are kernel-known in v2 (a UriLiteral's `refTo` IS its
/// identity, the way a literal's number is its value).
fn literal_value(node: &Json, typ: &str) -> Result<Option<EvalValue>, ExpressionError> {
    match typ {
        "kanonak.org/transformations/IntegerLiteral" => {
            Ok(as_number(node.get("integerLiteral")).map(EvalValue::Num))
        }
        "kanonak.org/transformations/DecimalLiteral" => {
            Ok(as_number(node.get("decimalLiteral")).map(EvalValue::Num))
        }
        "kanonak.org/transformations/BooleanLiteral" => {
            let b = match node.get("booleanLiteral") {
                Some(Json::Bool(b)) => *b,
                Some(Json::String(s)) => s == "true",
                _ => false,
            };
            Ok(Some(EvalValue::Num(boolnum(b))))
        }
        "kanonak.org/transformations/StringLiteral" => match node.get("stringLiteral") {
            Some(Json::String(s)) => Ok(Some(EvalValue::Str(s.clone()))),
            _ => err("StringLiteral is missing stringLiteral"),
        },
        "kanonak.org/transformations/UriLiteral" => match node.get("refTo") {
            Some(Json::String(s)) if !s.is_empty() => Ok(Some(EvalValue::Ref(s.clone()))),
            _ => err("UriLiteral is missing refTo"),
        },
        _ => Ok(None),
    }
}

fn as_number(v: Option<&Json>) -> Option<f64> {
    match v {
        Some(Json::Number(n)) => n.as_f64(),
        Some(Json::String(s)) => s.parse::<f64>().ok(),
        _ => None,
    }
}

/// Stringify one Join element. Numbers follow the ECMAScript number-to-string
/// rule RFC 8785 pins (integers without a decimal point); refs stringify to
/// their LOCAL name; a nested list is a loud computation error.
fn join_element(v: &EvalValue) -> Result<String, ExpressionError> {
    match v {
        EvalValue::Str(s) => Ok(s.clone()),
        EvalValue::Num(n) => Ok(format_number(*n)),
        EvalValue::Ref(r) => Ok(r.rsplit('/').next().unwrap_or(r).to_string()),
        EvalValue::List(_) => err("Join cannot stringify a nested list"),
    }
}

/// ECMAScript-style number formatting for the always-finite domain: integral
/// values print without a decimal point; everything else via Rust's shortest
/// round-trip formatting (Ryū), which matches ECMAScript for the values the
/// parity vectors exercise.
fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e21 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// `IsSet`: the empty list (absence) and the empty string are unset;
/// everything else — including 0 — is set.
fn is_set(v: &EvalValue) -> bool {
    match v {
        EvalValue::Str(s) => !s.is_empty(),
        EvalValue::List(l) => !l.is_empty(),
        _ => true,
    }
}

fn is_list_fold(typ: &str) -> bool {
    matches!(
        typ,
        "kanonak.org/transformations/Count"
            | "kanonak.org/transformations/Sum"
            | "kanonak.org/transformations/Min"
            | "kanonak.org/transformations/Max"
            | "kanonak.org/transformations/Average"
            | "kanonak.org/transformations/Join"
            | "kanonak.org/transformations/Reverse"
    )
}

/// The `source`-operand folds for the non-iterating list family. Normative
/// semantics frozen from the SDK reference engine: Sum on empty = 0;
/// Min/Max/Average on empty = loud error; Count type-agnostic.
fn list_fold(typ: &str, list: Vec<EvalValue>, node: &Json) -> Result<EvalValue, ExpressionError> {
    match typ {
        "kanonak.org/transformations/Count" => Ok(EvalValue::Num(list.len() as f64)),
        "kanonak.org/transformations/Sum" => {
            let mut total = 0.0;
            for el in &list {
                total += require_num(el, "Sum")?;
            }
            Ok(EvalValue::Num(total))
        }
        "kanonak.org/transformations/Min" | "kanonak.org/transformations/Max" => {
            let name = if typ.ends_with("Min") { "Min" } else { "Max" };
            if list.is_empty() {
                return err(format!("{name} on an empty list is undefined; guard with IsSet"));
            }
            let mut best = require_num(&list[0], name)?;
            for el in &list[1..] {
                let n = require_num(el, name)?;
                if (name == "Min" && n < best) || (name == "Max" && n > best) {
                    best = n;
                }
            }
            Ok(EvalValue::Num(best))
        }
        "kanonak.org/transformations/Average" => {
            if list.is_empty() {
                return err("Average on an empty list is undefined; guard with IsSet");
            }
            let mut total = 0.0;
            for el in &list {
                total += require_num(el, "Average")?;
            }
            Ok(EvalValue::Num(total / list.len() as f64))
        }
        "kanonak.org/transformations/Join" => {
            let sep = node.get("separator").and_then(Json::as_str).unwrap_or("");
            let mut parts = Vec::with_capacity(list.len());
            for el in &list {
                parts.push(join_element(el)?);
            }
            Ok(EvalValue::Str(parts.join(sep)))
        }
        "kanonak.org/transformations/Reverse" => {
            let mut l = list;
            l.reverse();
            Ok(EvalValue::List(l))
        }
        _ => err(format!("No list fold for {typ}")),
    }
}

/// KindPredicates the kernel can answer over ITS value domain. `IsBoolean` /
/// `IsEmbedded` name host-only kinds and fall through to `resolve`.
fn kind_predicate(typ: &str, v: &EvalValue) -> Option<f64> {
    match typ {
        "kanonak.org/transformations/IsString" => Some(boolnum(matches!(v, EvalValue::Str(_)))),
        "kanonak.org/transformations/IsNumber" => Some(boolnum(matches!(v, EvalValue::Num(_)))),
        "kanonak.org/transformations/IsReference" => Some(boolnum(matches!(v, EvalValue::Ref(_)))),
        "kanonak.org/transformations/IsList" => Some(boolnum(matches!(v, EvalValue::List(_)))),
        _ => None,
    }
}

fn is_kind_predicate(typ: &str) -> bool {
    matches!(
        typ,
        "kanonak.org/transformations/IsString"
            | "kanonak.org/transformations/IsNumber"
            | "kanonak.org/transformations/IsReference"
            | "kanonak.org/transformations/IsList"
    )
}

// ---------------------------------------------------------------------------
// Matches — the pinned RE2-compatible XSD-regex subset.
// ---------------------------------------------------------------------------

/// Split the whole-pattern flag prefix — `(?i)`, `(?ims)` at position 0.
fn split_flag_prefix(pattern: &str) -> (String, &str) {
    let bytes = pattern.as_bytes();
    if bytes.len() >= 4 && bytes[0] == b'(' && bytes[1] == b'?' {
        let mut j = 2;
        while j < bytes.len() && matches!(bytes[j], b'i' | b'm' | b's') {
            j += 1;
        }
        if j > 2 && j < bytes.len() && bytes[j] == b')' {
            return (pattern[2..j].to_string(), &pattern[j + 1..]);
        }
    }
    (String::new(), pattern)
}

/// Validate a `Matches` pattern body against the pinned subset — the same
/// scanner as the reference kernel, so every port rejects the same
/// constructs with a loud error.
fn validate_matches_pattern(pattern: &str) -> Result<(), ExpressionError> {
    let fail = |what: &str| -> Result<(), ExpressionError> {
        err(format!(
            "Matches pattern is outside the pinned regex subset ({what}): {pattern}"
        ))
    };
    const ALLOWED: &[char] = &[
        'd', 'D', 'w', 'W', 's', 'S', 'b', 'B', 'n', 'r', 't', 'f', 'v', '.', '*', '+', '?', '(',
        ')', '[', ']', '{', '}', '|', '^', '$', '\\', '/',
    ];
    let chars: Vec<char> = pattern.chars().collect();
    let mut in_class = false;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            let e = match chars.get(i + 1) {
                Some(e) => *e,
                None => return fail("trailing backslash"),
            };
            if e == 'x' {
                let h1 = chars.get(i + 2).copied().unwrap_or('!');
                let h2 = chars.get(i + 3).copied().unwrap_or('!');
                if h1 == '{' {
                    return fail("\\x{…} escape");
                }
                if !h1.is_ascii_hexdigit() || !h2.is_ascii_hexdigit() {
                    return fail("\\x escape must be \\xHH");
                }
                i += 4;
                continue;
            }
            if e.is_ascii_digit() {
                return fail(&format!("backreference or octal escape \\{e}"));
            }
            if e == 'p' || e == 'P' {
                return fail(&format!("unicode property class \\{e}{{…}}"));
            }
            if e == 'k' {
                return fail("named backreference \\k");
            }
            if e == 'u' {
                return fail("\\u escape");
            }
            if e == '-' {
                if !in_class {
                    return fail("\\- outside a character class");
                }
                i += 2;
                continue;
            }
            if (e == 'b' || e == 'B' || e == 'D' || e == 'W' || e == 'S') && in_class {
                return fail(&format!("\\{e} inside a character class"));
            }
            if !ALLOWED.contains(&e) {
                return fail(&format!("escape \\{e}"));
            }
            i += 2;
            continue;
        }
        if in_class {
            if c == ']' {
                in_class = false;
            } else if c == '&' && chars.get(i + 1) == Some(&'&') {
                return fail("character-class intersection &&");
            } else if c == '[' && chars.get(i + 1) == Some(&':') {
                return fail("POSIX class [[:…:]]");
            }
            i += 1;
            continue;
        }
        if c == '[' {
            in_class = true;
            i += 1;
            continue;
        }
        if c == '(' && chars.get(i + 1) == Some(&'?') {
            // Only (?: survives mid-pattern; the flag prefix is split off
            // before this scanner runs.
            if chars.get(i + 2) != Some(&':') {
                let after = chars.get(i + 2).map(|c| c.to_string()).unwrap_or_default();
                return fail(&format!("group construct (?{after}"));
            }
            i += 3;
            continue;
        }
        if c == '{' {
            // A bare `{` must start a valid quantifier — the SCANNER enforces
            // this uniformly (a literal brace is written \{); engines disagree
            // on the lenient literal reading.
            let mut j = i + 1;
            let digits = |k: &mut usize| {
                let start = *k;
                while *k < chars.len() && chars[*k].is_ascii_digit() {
                    *k += 1;
                }
                *k > start
            };
            let mut ok = digits(&mut j);
            if ok && chars.get(j) == Some(&',') {
                j += 1;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
            }
            ok = ok && chars.get(j) == Some(&'}');
            if !ok {
                return fail("bare '{' that is not a quantifier (write \\{)");
            }
        }
        i += 1;
    }
    if in_class {
        return fail("unterminated character class");
    }
    Ok(())
}

/// Apply the pinned ASCII expansions of the shorthand classes (the dialect
/// DEFINES `\d` `\w` `\s` by these), and rewrite `\b`/`\B` to the ASCII word
/// boundary this engine expresses as `(?-u:\b)` — its native `\b` is Unicode.
fn expand_shorthand_classes(body: &str) -> String {
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::with_capacity(body.len() + 16);
    let mut in_class = false;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            let e = chars[i + 1]; // subset-validated: never a trailing backslash
            let expansion: Option<&str> = if in_class {
                match e {
                    'd' => Some("0-9"),
                    'w' => Some("0-9A-Za-z_"),
                    's' => Some(" \\t\\n\\r\\f\\x0B"),
                    _ => None,
                }
            } else {
                match e {
                    'd' => Some("[0-9]"),
                    'D' => Some("[^0-9]"),
                    'w' => Some("[0-9A-Za-z_]"),
                    'W' => Some("[^0-9A-Za-z_]"),
                    's' => Some("[ \\t\\n\\r\\f\\x0B]"),
                    'S' => Some("[^ \\t\\n\\r\\f\\x0B]"),
                    'b' => Some("(?-u:\\b)"),
                    'B' => Some("(?-u:\\B)"),
                    _ => None,
                }
            };
            match expansion {
                Some(x) => out.push_str(x),
                None => {
                    out.push('\\');
                    out.push(e);
                }
            }
            i += 2;
            continue;
        }
        if !in_class && c == '[' {
            in_class = true;
        } else if in_class && c == ']' {
            in_class = false;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Compile + test under fn:matches semantics: UNANCHORED. `.` and quantifiers
/// count code points (this engine's native model); flags translate via
/// RegexBuilder.
fn matches_pattern(input: &str, pattern: &str) -> Result<bool, ExpressionError> {
    let (flags, body) = split_flag_prefix(pattern);
    validate_matches_pattern(body)?;
    let expanded = expand_shorthand_classes(body);
    let re = regex::RegexBuilder::new(&expanded)
        .case_insensitive(flags.contains('i'))
        .multi_line(flags.contains('m'))
        .dot_matches_new_line(flags.contains('s'))
        .build()
        .map_err(|_| ExpressionError(format!("Matches pattern does not compile: {pattern}")))?;
    Ok(re.is_match(input))
}

// ---------------------------------------------------------------------------
// Ordered comparisons (unchanged from v1).
// ---------------------------------------------------------------------------

fn identity_of<C>(
    node: &Json,
    ctx: &mut C,
    options: Option<&EvalOptions<'_, C>>,
) -> Result<String, ExpressionError> {
    let typ = node_type(node)?;
    if typ == "kanonak.org/transformations/UriLiteral" {
        return match node.get("refTo") {
            Some(Json::String(s)) if !s.is_empty() => Ok(s.clone()),
            _ => err("UriLiteral is missing refTo"),
        };
    }
    match options.and_then(|o| o.resolve_ref) {
        Some(rr) => rr(node, ctx),
        None => err(format!("No resolveRef supplied for identity leaf '{typ}'")),
    }
}

fn fold_ordered<C>(
    node: &Json,
    typ: &str,
    ctx: &mut C,
    options: Option<&EvalOptions<'_, C>>,
) -> Result<(f64, String, String), ExpressionError> {
    let via = match node.get("viaProperty").and_then(Json::as_str) {
        Some(v) if !v.is_empty() => v,
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
        boolnum(
            closure
                .get(&left)
                .map(|reach| reach.iter().any(|m| m == &right))
                .unwrap_or(false),
        )
    };
    Ok((value, left, right))
}

// ---------------------------------------------------------------------------
// The fold.
// ---------------------------------------------------------------------------

type Frames = Vec<(String, EvalValue)>;

fn bound_value(frames: &Frames, name: &str) -> Option<EvalValue> {
    frames
        .iter()
        .rev()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.clone())
}

/// Evaluate an expression tree to a value (no ordered-comparison context).
pub fn evaluate<C>(
    node: &Json,
    ctx: &mut C,
    resolve: Resolve<'_, C>,
) -> Result<EvalValue, ExpressionError> {
    go(node, ctx, resolve, None, &mut Vec::new())
}

/// Evaluate with ordered-comparison context (closures + identity resolution).
pub fn evaluate_with_options<C>(
    node: &Json,
    ctx: &mut C,
    resolve: Resolve<'_, C>,
    options: &EvalOptions<'_, C>,
) -> Result<EvalValue, ExpressionError> {
    go(node, ctx, resolve, Some(options), &mut Vec::new())
}

fn source_list<C>(
    node: &Json,
    typ: &str,
    ctx: &mut C,
    resolve: Resolve<'_, C>,
    options: Option<&EvalOptions<'_, C>>,
    frames: &mut Frames,
) -> Result<Vec<EvalValue>, ExpressionError> {
    let v = go(operand(node, typ, "source")?, ctx, resolve, options, frames)?;
    Ok(match v {
        EvalValue::List(l) => l,
        scalar => vec![scalar],
    })
}

fn go<C>(
    node: &Json,
    ctx: &mut C,
    resolve: Resolve<'_, C>,
    options: Option<&EvalOptions<'_, C>>,
    frames: &mut Frames,
) -> Result<EvalValue, ExpressionError> {
    let typ = node_type(node)?;

    if let Some(arity) = operator_arity(typ) {
        match arity {
            Arity::Unary { operand: key } => {
                let x = go(operand(node, typ, key)?, ctx, resolve, options, frames)?;
                return Ok(EvalValue::Num(unary(typ, require_num(&x, typ)?)?));
            }
            Arity::Binary { left, right } => {
                let a = go(operand(node, typ, left)?, ctx, resolve, options, frames)?;
                let b = go(operand(node, typ, right)?, ctx, resolve, options, frames)?;
                if typ == "kanonak.org/transformations/Equals" {
                    return Ok(EvalValue::Num(boolnum(values_equal(&a, &b))));
                }
                if is_order_comparison(typ) {
                    // Predicate: non-numeric operands fail CLOSED.
                    return Ok(match (&a, &b) {
                        (EvalValue::Num(x), EvalValue::Num(y)) => {
                            EvalValue::Num(binary_order(typ, *x, *y).unwrap())
                        }
                        _ => EvalValue::Num(0.0),
                    });
                }
                // Arithmetic: computation — fail LOUD on non-numbers.
                return Ok(EvalValue::Num(binary_arith(
                    typ,
                    require_num(&a, typ)?,
                    require_num(&b, typ)?,
                )?));
            }
            Arity::Nary { operands } => {
                let items = match node.get(operands) {
                    Some(Json::Array(items)) => items,
                    _ => return err(format!("{typ} expects an '{operands}' list")),
                };
                let is_and = typ == "kanonak.org/transformations/And";
                // Short-circuit; empty And vacuously true, empty Or false.
                for item in items {
                    let v = go(item, ctx, resolve, options, frames)?;
                    let t = truthy(require_num(&v, typ)?);
                    if is_and && !t {
                        return Ok(EvalValue::Num(0.0));
                    }
                    if !is_and && t {
                        return Ok(EvalValue::Num(1.0));
                    }
                }
                return Ok(EvalValue::Num(boolnum(is_and)));
            }
            Arity::Ternary { a, b, c } => {
                // Only Clip today: clamp clipValue into [clipLower, clipUpper].
                let v = require_num(&go(operand(node, typ, a)?, ctx, resolve, options, frames)?, typ)?;
                let lo = require_num(&go(operand(node, typ, b)?, ctx, resolve, options, frames)?, typ)?;
                let hi = require_num(&go(operand(node, typ, c)?, ctx, resolve, options, frames)?, typ)?;
                return Ok(EvalValue::Num(v.max(lo).min(hi)));
            }
        }
    }

    if typ == "kanonak.org/transformations/Not" {
        let x = go(operand(node, typ, "operand")?, ctx, resolve, options, frames)?;
        return Ok(EvalValue::Num(boolnum(!truthy(require_num(&x, typ)?))));
    }

    if typ == "kanonak.org/transformations/IsAtLeast"
        || typ == "kanonak.org/transformations/Dominates"
    {
        let (value, _, _) = fold_ordered(node, typ, ctx, options)?;
        return Ok(EvalValue::Num(value));
    }

    if is_list_fold(typ) {
        let list = source_list(node, typ, ctx, resolve, options, frames)?;
        return list_fold(typ, list, node);
    }

    if let Some(body_key) = iterator_body(typ) {
        let loop_var = match node.get("loopVar").and_then(Json::as_str) {
            Some(v) if !v.is_empty() => v.to_string(),
            _ => return err(format!("{typ} is missing loopVar")),
        };
        let list = source_list(node, typ, ctx, resolve, options, frames)?;
        let body = operand(node, typ, body_key)?;
        let mut out: Vec<EvalValue> = Vec::new();
        for el in list {
            frames.push((loop_var.clone(), el.clone()));
            let v = go(body, ctx, resolve, options, frames);
            frames.pop();
            let v = v?;
            match typ {
                "kanonak.org/transformations/Filter" => {
                    if truthy(require_num(&v, "Filter predicate")?) {
                        out.push(el);
                    }
                }
                "kanonak.org/transformations/ForEach" => match v {
                    // Flatten one level; an empty list contributes nothing —
                    // the absence rule doing the reference engine's skip.
                    EvalValue::List(items) => out.extend(items),
                    scalar => out.push(scalar),
                },
                _ => out.push(v),
            }
        }
        return Ok(EvalValue::List(out));
    }

    if typ == "kanonak.org/transformations/Contains" {
        let hay = go(operand(node, typ, "haystack")?, ctx, resolve, options, frames)?;
        let needle = go(operand(node, typ, "needle")?, ctx, resolve, options, frames)?;
        let list = match hay {
            EvalValue::List(l) => l,
            scalar => vec![scalar],
        };
        return Ok(EvalValue::Num(boolnum(
            list.iter().any(|el| values_equal(el, &needle)),
        )));
    }

    if typ == "kanonak.org/transformations/IsSet" {
        let v = go(operand(node, typ, "checkExpr")?, ctx, resolve, options, frames)?;
        return Ok(EvalValue::Num(boolnum(is_set(&v))));
    }

    if typ == "kanonak.org/transformations/ListItemAt" {
        let list = source_list(node, typ, ctx, resolve, options, frames)?;
        let idx = go(operand(node, typ, "itemIndex")?, ctx, resolve, options, frames)?;
        let n = match idx {
            EvalValue::Num(n) if n.fract() == 0.0 && n >= 0.0 => n as usize,
            _ => return err("ListItemAt itemIndex must be a non-negative integer"),
        };
        // Past the end is ABSENCE (the empty list); guard with IsSet.
        return Ok(list.into_iter().nth(n).unwrap_or(EvalValue::List(Vec::new())));
    }

    if typ == "kanonak.org/transformations/Matches" {
        let src = go(operand(node, typ, "matchSource")?, ctx, resolve, options, frames)?;
        let s = match &src {
            EvalValue::Str(s) => s,
            other => {
                return err(format!(
                    "Matches requires a string matchSource, got {}",
                    other.kind()
                ))
            }
        };
        let pattern = match node.get("pattern").and_then(Json::as_str) {
            Some(p) => p,
            None => return err("Matches is missing pattern"),
        };
        return Ok(EvalValue::Num(boolnum(matches_pattern(s, pattern)?)));
    }

    if is_kind_predicate(typ) {
        let v = go(operand(node, typ, "kindCheck")?, ctx, resolve, options, frames)?;
        return Ok(EvalValue::Num(kind_predicate(typ, &v).unwrap()));
    }

    if let Some(lit) = literal_value(node, typ)? {
        return Ok(lit);
    }

    // A VarRef naming a lexically-enclosing loopVar is the kernel's own bound
    // variable — the ONLY leaf the kernel answers. Everything else is the
    // caller's; recursion from inside `resolve` re-enters WITHOUT frames.
    if typ == "kanonak.org/transformations/VarRef" {
        if let Some(name) = node.get("varName").and_then(Json::as_str) {
            if let Some(v) = bound_value(frames, name) {
                return Ok(v);
            }
        }
    }

    resolve(node, ctx, &mut |n, c| {
        go(n, c, resolve, options, &mut Vec::new())
    })
}

// ---------------------------------------------------------------------------
// explain — the verdict tree.
// ---------------------------------------------------------------------------

/// Evaluate and return the verdict tree. The root's `value` is exactly what
/// [`evaluate`] returns for the same inputs; the conformance suite runs every
/// vector through both and requires agreement. Errors propagate exactly as in
/// `evaluate` — a failed evaluation yields an error, not a partial trace.
pub fn explain<C>(
    node: &Json,
    ctx: &mut C,
    resolve: Resolve<'_, C>,
    options: Option<&EvalOptions<'_, C>>,
) -> Result<TraceNode, ExpressionError> {
    trace(node, ctx, resolve, options, &mut Vec::new())
}

fn leaf(typ: &str, value: EvalValue) -> TraceNode {
    TraceNode {
        typ: typ.to_string(),
        value,
        children: Vec::new(),
        left_ref: None,
        right_ref: None,
    }
}

fn parent(typ: &str, value: EvalValue, children: Vec<TraceNode>) -> TraceNode {
    TraceNode {
        typ: typ.to_string(),
        value,
        children,
        left_ref: None,
        right_ref: None,
    }
}

fn trace<C>(
    node: &Json,
    ctx: &mut C,
    resolve: Resolve<'_, C>,
    options: Option<&EvalOptions<'_, C>>,
    frames: &mut Frames,
) -> Result<TraceNode, ExpressionError> {
    let typ = node_type(node)?;

    if let Some(arity) = operator_arity(typ) {
        match arity {
            Arity::Unary { operand: key } => {
                let x = trace(operand(node, typ, key)?, ctx, resolve, options, frames)?;
                let v = unary(typ, require_num(&x.value, typ)?)?;
                return Ok(parent(typ, EvalValue::Num(v), vec![x]));
            }
            Arity::Binary { left, right } => {
                let a = trace(operand(node, typ, left)?, ctx, resolve, options, frames)?;
                let b = trace(operand(node, typ, right)?, ctx, resolve, options, frames)?;
                let value = if typ == "kanonak.org/transformations/Equals" {
                    EvalValue::Num(boolnum(values_equal(&a.value, &b.value)))
                } else if is_order_comparison(typ) {
                    match (&a.value, &b.value) {
                        (EvalValue::Num(x), EvalValue::Num(y)) => {
                            EvalValue::Num(binary_order(typ, *x, *y).unwrap())
                        }
                        _ => EvalValue::Num(0.0),
                    }
                } else {
                    EvalValue::Num(binary_arith(
                        typ,
                        require_num(&a.value, typ)?,
                        require_num(&b.value, typ)?,
                    )?)
                };
                return Ok(parent(typ, value, vec![a, b]));
            }
            Arity::Nary { operands } => {
                let items = match node.get(operands) {
                    Some(Json::Array(items)) => items,
                    _ => return err(format!("{typ} expects an '{operands}' list")),
                };
                let is_and = typ == "kanonak.org/transformations/And";
                let mut children = Vec::new();
                for item in items {
                    let child = trace(item, ctx, resolve, options, frames)?;
                    let t = truthy(require_num(&child.value, typ)?);
                    children.push(child);
                    // Same short-circuit as `evaluate`: later operands never
                    // run and never appear in the trace.
                    if is_and && !t {
                        return Ok(parent(typ, EvalValue::Num(0.0), children));
                    }
                    if !is_and && t {
                        return Ok(parent(typ, EvalValue::Num(1.0), children));
                    }
                }
                return Ok(parent(typ, EvalValue::Num(boolnum(is_and)), children));
            }
            Arity::Ternary { a, b, c } => {
                let tv = trace(operand(node, typ, a)?, ctx, resolve, options, frames)?;
                let tlo = trace(operand(node, typ, b)?, ctx, resolve, options, frames)?;
                let thi = trace(operand(node, typ, c)?, ctx, resolve, options, frames)?;
                let v = require_num(&tv.value, typ)?;
                let lo = require_num(&tlo.value, typ)?;
                let hi = require_num(&thi.value, typ)?;
                return Ok(parent(typ, EvalValue::Num(v.max(lo).min(hi)), vec![tv, tlo, thi]));
            }
        }
    }

    if typ == "kanonak.org/transformations/Not" {
        let x = trace(operand(node, typ, "operand")?, ctx, resolve, options, frames)?;
        let v = boolnum(!truthy(require_num(&x.value, typ)?));
        return Ok(parent(typ, EvalValue::Num(v), vec![x]));
    }

    if typ == "kanonak.org/transformations/IsAtLeast"
        || typ == "kanonak.org/transformations/Dominates"
    {
        let (value, l, r) = fold_ordered(node, typ, ctx, options)?;
        return Ok(TraceNode {
            typ: typ.to_string(),
            value: EvalValue::Num(value),
            children: Vec::new(),
            left_ref: Some(l),
            right_ref: Some(r),
        });
    }

    if is_list_fold(typ) {
        let src = trace(operand(node, typ, "source")?, ctx, resolve, options, frames)?;
        let list = match &src.value {
            EvalValue::List(l) => l.clone(),
            scalar => vec![scalar.clone()],
        };
        let value = list_fold(typ, list, node)?;
        return Ok(parent(typ, value, vec![src]));
    }

    if let Some(body_key) = iterator_body(typ) {
        let loop_var = match node.get("loopVar").and_then(Json::as_str) {
            Some(v) if !v.is_empty() => v.to_string(),
            _ => return err(format!("{typ} is missing loopVar")),
        };
        let src = trace(operand(node, typ, "source")?, ctx, resolve, options, frames)?;
        let list = match &src.value {
            EvalValue::List(l) => l.clone(),
            scalar => vec![scalar.clone()],
        };
        let body = operand(node, typ, body_key)?;
        let mut children = vec![src];
        let mut out: Vec<EvalValue> = Vec::new();
        for el in list {
            frames.push((loop_var.clone(), el.clone()));
            let bt = trace(body, ctx, resolve, options, frames);
            frames.pop();
            let bt = bt?;
            let v = bt.value.clone();
            children.push(bt);
            match typ {
                "kanonak.org/transformations/Filter" => {
                    if truthy(require_num(&v, "Filter predicate")?) {
                        out.push(el);
                    }
                }
                "kanonak.org/transformations/ForEach" => match v {
                    EvalValue::List(items) => out.extend(items),
                    scalar => out.push(scalar),
                },
                _ => out.push(v),
            }
        }
        return Ok(parent(typ, EvalValue::List(out), children));
    }

    if typ == "kanonak.org/transformations/Contains" {
        let hay = trace(operand(node, typ, "haystack")?, ctx, resolve, options, frames)?;
        let needle = trace(operand(node, typ, "needle")?, ctx, resolve, options, frames)?;
        let list = match &hay.value {
            EvalValue::List(l) => l.clone(),
            scalar => vec![scalar.clone()],
        };
        let v = boolnum(list.iter().any(|el| values_equal(el, &needle.value)));
        return Ok(parent(typ, EvalValue::Num(v), vec![hay, needle]));
    }

    if typ == "kanonak.org/transformations/IsSet" {
        let x = trace(operand(node, typ, "checkExpr")?, ctx, resolve, options, frames)?;
        let v = boolnum(is_set(&x.value));
        return Ok(parent(typ, EvalValue::Num(v), vec![x]));
    }

    if typ == "kanonak.org/transformations/ListItemAt" {
        let src = trace(operand(node, typ, "source")?, ctx, resolve, options, frames)?;
        let idx = trace(operand(node, typ, "itemIndex")?, ctx, resolve, options, frames)?;
        let list = match &src.value {
            EvalValue::List(l) => l.clone(),
            scalar => vec![scalar.clone()],
        };
        let n = match &idx.value {
            EvalValue::Num(n) if n.fract() == 0.0 && *n >= 0.0 => *n as usize,
            _ => return err("ListItemAt itemIndex must be a non-negative integer"),
        };
        let value = list.into_iter().nth(n).unwrap_or(EvalValue::List(Vec::new()));
        return Ok(parent(typ, value, vec![src, idx]));
    }

    if typ == "kanonak.org/transformations/Matches" {
        let src = trace(operand(node, typ, "matchSource")?, ctx, resolve, options, frames)?;
        let s = match &src.value {
            EvalValue::Str(s) => s.clone(),
            other => {
                return err(format!(
                    "Matches requires a string matchSource, got {}",
                    other.kind()
                ))
            }
        };
        let pattern = match node.get("pattern").and_then(Json::as_str) {
            Some(p) => p,
            None => return err("Matches is missing pattern"),
        };
        let v = boolnum(matches_pattern(&s, pattern)?);
        return Ok(parent(typ, EvalValue::Num(v), vec![src]));
    }

    if is_kind_predicate(typ) {
        let x = trace(operand(node, typ, "kindCheck")?, ctx, resolve, options, frames)?;
        let v = kind_predicate(typ, &x.value).unwrap();
        return Ok(parent(typ, EvalValue::Num(v), vec![x]));
    }

    if let Some(lit) = literal_value(node, typ)? {
        return Ok(leaf(typ, lit));
    }

    if typ == "kanonak.org/transformations/VarRef" {
        if let Some(name) = node.get("varName").and_then(Json::as_str) {
            if let Some(v) = bound_value(frames, name) {
                return Ok(leaf(typ, v));
            }
        }
    }

    let v = resolve(node, ctx, &mut |n, c| {
        go(n, c, resolve, options, &mut Vec::new())
    })?;
    Ok(leaf(typ, v))
}
