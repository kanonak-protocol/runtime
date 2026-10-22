// Package canonical computes the Kanonak canonical form + content hash
// (canonicalFormVersion "1"). An independent conformant port of
// kanonak.org/canonical-form, verified byte-for-byte against the golden vectors.
package canonical

import (
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"fmt"
	"math"
	"math/big"
	"regexp"
	"sort"
	"strconv"
	"strings"

	"golang.org/x/text/unicode/norm"
)

const CanonicalFormVersion = "1"

// ---------------------------------------------------------------------------
// Carriers
// ---------------------------------------------------------------------------

type Carrier string

const (
	CarrierInteger      Carrier = "integer"
	CarrierDecimal      Carrier = "decimal"
	CarrierDouble       Carrier = "double"
	CarrierFloat        Carrier = "float"
	CarrierBoolean      Carrier = "boolean"
	CarrierString       Carrier = "string"
	CarrierAnyURI       Carrier = "anyURI"
	CarrierLangString   Carrier = "langString"
	CarrierDateTime     Carrier = "dateTime"
	CarrierDate         Carrier = "date"
	CarrierTime         Carrier = "time"
	CarrierHexBinary    Carrier = "hexBinary"
	CarrierBase64Binary Carrier = "base64Binary"
)

var xsdCarrier = map[string]Carrier{
	"integer": CarrierInteger, "long": CarrierInteger, "int": CarrierInteger,
	"short": CarrierInteger, "byte": CarrierInteger, "unsignedLong": CarrierInteger,
	"unsignedInt": CarrierInteger, "unsignedShort": CarrierInteger, "unsignedByte": CarrierInteger,
	"nonNegativeInteger": CarrierInteger, "positiveInteger": CarrierInteger,
	"nonPositiveInteger": CarrierInteger, "negativeInteger": CarrierInteger,
	"decimal": CarrierDecimal, "double": CarrierDouble, "float": CarrierFloat,
	"boolean": CarrierBoolean, "string": CarrierString, "normalizedString": CarrierString,
	"token": CarrierString, "anyURI": CarrierAnyURI, "dateTime": CarrierDateTime,
	"date": CarrierDate, "time": CarrierTime, "hexBinary": CarrierHexBinary,
	"base64Binary": CarrierBase64Binary,
}

// CarrierOf returns the carrier for a datatype URI, or ok=false (out-of-set).
// A URI without the three-segment coordinate shape routes to no carrier.
func CarrierOf(uri string) (Carrier, bool) {
	key, ok := LenientVersionlessKey(uri)
	if !ok {
		return "", false
	}
	if key == "kanonak.org/core-rdf/langString" {
		return CarrierLangString, true
	}
	const prefix = "kanonak.org/core-xsd/"
	if !strings.HasPrefix(key, prefix) {
		return "", false
	}
	c, ok := xsdCarrier[key[len(prefix):]]
	return c, ok
}

// ---------------------------------------------------------------------------
// Per-carrier canonical lexical forms
// ---------------------------------------------------------------------------

var (
	integerRe  = regexp.MustCompile(`^[+-]?\d+$`)
	decimalRe  = regexp.MustCompile(`^([+-]?)(\d*)(?:\.(\d*))?$`)
	ieeeRe     = regexp.MustCompile(`^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$`)
	hexRe      = regexp.MustCompile(`^([0-9A-Fa-f]{2})*$`)
	base64Re   = regexp.MustCompile(`^[A-Za-z0-9+/]*={0,2}$`)
	dateTimeRe = regexp.MustCompile(`^(-?\d{4,})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})?$`)
	dateRe     = regexp.MustCompile(`^(-?\d{4,})-(\d{2})-(\d{2})(Z|[+-]\d{2}:\d{2})?$`)
	timeRe     = regexp.MustCompile(`^(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})?$`)
	wsRe       = regexp.MustCompile(`\s+`)
)

func CanonicalInteger(raw string) (string, error) {
	t := strings.TrimSpace(raw)
	if !integerRe.MatchString(t) {
		return "", fmt.Errorf("canonicalInteger: %q invalid", raw)
	}
	sign := ""
	digits := t
	if strings.HasPrefix(t, "-") {
		sign, digits = "-", t[1:]
	} else if strings.HasPrefix(t, "+") {
		digits = t[1:]
	}
	stripped := strings.TrimLeft(digits, "0")
	if stripped == "" {
		stripped = "0"
	}
	if stripped == "0" {
		return "0", nil
	}
	return sign + stripped, nil
}

func CanonicalDecimal(raw string) (string, error) {
	t := strings.TrimSpace(raw)
	m := decimalRe.FindStringSubmatch(t)
	if m == nil || (m[2] == "" && m[3] == "") {
		return "", fmt.Errorf("canonicalDecimal: %q invalid", raw)
	}
	sign := ""
	if m[1] == "-" {
		sign = "-"
	}
	intPart := strings.TrimLeft(m[2], "0")
	if intPart == "" {
		intPart = "0"
	}
	fracPart := strings.TrimRight(m[3], "0")
	magnitude := intPart
	if fracPart != "" {
		magnitude = intPart + "." + fracPart
	}
	if magnitude == "0" {
		return "0", nil
	}
	return sign + magnitude, nil
}

func canonicalIeee(raw string, single bool, label string) (string, error) {
	t := strings.TrimSpace(raw)
	switch t {
	case "NaN":
		return "NaN", nil
	case "INF":
		return "INF", nil
	case "-INF":
		return "-INF", nil
	}
	if !ieeeRe.MatchString(t) {
		return "", fmt.Errorf("canonical%s: %q invalid", label, raw)
	}
	n, err := strconv.ParseFloat(t, 64)
	if err != nil {
		return "", fmt.Errorf("canonical%s: %q invalid", label, raw)
	}
	// A float is its value rounded to 32 bits, then written as that double (runtime#35).
	if single {
		n = float64(float32(n))
	}
	if math.IsInf(n, 0) || math.IsNaN(n) {
		return "", fmt.Errorf("canonical%s: %q out of range", label, raw)
	}
	if n == 0 {
		return "0", nil
	}
	return ecmaScriptDoubleToken(n), nil
}

// ecmaScriptDoubleToken is ECMAScript's Number::toString of a finite, non-zero double: RFC 8785's number
// notation. FormatFloat gives the shortest round-tripping digits; only the notation is re-derived, plain while
// -6 < n <= 21.
func ecmaScriptDoubleToken(d float64) string {
	sign := ""
	if d < 0 {
		sign = "-"
		d = -d
	}
	mantissa, exponentText, _ := strings.Cut(strconv.FormatFloat(d, 'e', -1, 64), "e")
	exponent, err := strconv.Atoi(exponentText)
	if err != nil {
		panic(fmt.Sprintf("strconv.FormatFloat wrote exponent %q, not an integer", exponentText))
	}
	intPart, fracPart, _ := strings.Cut(mantissa, ".")
	combined := intPart + fracPart
	trimmed := strings.TrimLeft(combined, "0")
	lead := len(combined) - len(trimmed)
	digits := strings.TrimRight(trimmed, "0")
	n := len(intPart) - lead + exponent
	k := len(digits)
	var body string
	switch {
	case k <= n && n <= 21:
		body = digits + strings.Repeat("0", n-k)
	case 0 < n && n <= 21:
		body = digits[:n] + "." + digits[n:]
	case -6 < n && n <= 0:
		body = "0." + strings.Repeat("0", -n) + digits
	default:
		e := n - 1
		head := digits
		if k > 1 {
			head = digits[:1] + "." + digits[1:]
		}
		exponentSign := "+"
		if e < 0 {
			exponentSign = "-"
			e = -e
		}
		body = head + "e" + exponentSign + strconv.Itoa(e)
	}
	return sign + body
}

func CanonicalDouble(raw string) (string, error) { return canonicalIeee(raw, false, "Double") }
func CanonicalFloat(raw string) (string, error)  { return canonicalIeee(raw, true, "Float") }

func CanonicalBoolean(raw string) (string, error) {
	switch strings.TrimSpace(raw) {
	case "true", "1":
		return "true", nil
	case "false", "0":
		return "false", nil
	}
	return "", fmt.Errorf("canonicalBoolean: %q invalid", raw)
}

func CanonicalString(raw string) string { return norm.NFC.String(raw) }

func CanonicalLanguageTag(tag string) (string, error) {
	subs := strings.Split(strings.TrimSpace(tag), "-")
	if len(subs) == 0 || subs[0] == "" {
		return "", fmt.Errorf("canonicalLanguageTag: %q invalid", tag)
	}
	for i, sub := range subs {
		switch {
		case i == 0:
			subs[i] = strings.ToLower(sub)
		case len(sub) == 4 && isAlpha(sub):
			subs[i] = strings.ToUpper(sub[:1]) + strings.ToLower(sub[1:])
		case len(sub) == 2 && isAlpha(sub):
			subs[i] = strings.ToUpper(sub)
		default:
			subs[i] = strings.ToLower(sub)
		}
	}
	return strings.Join(subs, "-"), nil
}

func isAlpha(s string) bool {
	for _, c := range s {
		if !((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z')) {
			return false
		}
	}
	return true
}

func CanonicalHexBinary(raw string) (string, error) {
	t := strings.TrimSpace(raw)
	if !hexRe.MatchString(t) {
		return "", fmt.Errorf("canonicalHexBinary: %q invalid", raw)
	}
	return strings.ToUpper(t), nil
}

func CanonicalBase64(raw string) (string, error) {
	stripped := wsRe.ReplaceAllString(raw, "")
	if !base64Re.MatchString(stripped) || len(stripped)%4 != 0 {
		return "", fmt.Errorf("canonicalBase64: %q invalid", raw)
	}
	bytes, err := base64.StdEncoding.DecodeString(stripped)
	if err != nil {
		return "", fmt.Errorf("canonicalBase64: %q invalid", raw)
	}
	return base64.StdEncoding.EncodeToString(bytes), nil
}

// -- temporal: XSD 1.1's rules, on the proleptic Gregorian calendar with a year 0 ----
// A year of any length is an exact big.Int, so a UTC shift across a year boundary has no range limit
// (runtime#36).

var daysInMonthTable = [12]int{31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31}

func isLeapYear(year *big.Int) bool {
	r := new(big.Int).Mod(year, big.NewInt(400)).Int64()
	return r%4 == 0 && (r%100 != 0 || r == 0)
}

func daysInMonth(year *big.Int, month int) int {
	if month == 2 && isLeapYear(year) {
		return 29
	}
	return daysInMonthTable[month-1]
}

// yearText is a year at least four digits wide, its sign kept; year 0 has none.
func yearText(year *big.Int) string {
	digits := new(big.Int).Abs(year).String()
	if len(digits) < 4 {
		digits = strings.Repeat("0", 4-len(digits)) + digits
	}
	if year.Sign() < 0 {
		return "-" + digits
	}
	return digits
}

type civil struct {
	year       *big.Int
	month, day int
}

func twoDigits(field string) int {
	n, err := strconv.Atoi(field)
	if err != nil {
		panic(fmt.Sprintf("the pattern matched two digits, not %q", field))
	}
	return n
}

func civilOf(yy, mo, dd, what, raw string) (civil, error) {
	year, ok := new(big.Int).SetString(yy, 10)
	if !ok {
		panic(fmt.Sprintf("the pattern matched a year, not %q", yy))
	}
	month, day := twoDigits(mo), twoDigits(dd)
	if month < 1 || month > 12 {
		return civil{}, fmt.Errorf("%s: %q has month %s; a month is 01 to 12", what, raw, mo)
	}
	if day < 1 || day > daysInMonth(year, month) {
		return civil{}, fmt.Errorf("%s: %q has day %s, which month %s of year %s doesn't have", what, raw, dd, mo, yearText(year))
	}
	return civil{year, month, day}, nil
}

func nextDay(c civil) civil {
	if c.day < daysInMonth(c.year, c.month) {
		return civil{c.year, c.month, c.day + 1}
	}
	if c.month < 12 {
		return civil{c.year, c.month + 1, 1}
	}
	return civil{new(big.Int).Add(c.year, big.NewInt(1)), 1, 1}
}

func previousDay(c civil) civil {
	if c.day > 1 {
		return civil{c.year, c.month, c.day - 1}
	}
	if c.month > 1 {
		return civil{c.year, c.month - 1, daysInMonth(c.year, c.month-1)}
	}
	return civil{new(big.Int).Sub(c.year, big.NewInt(1)), 12, 31}
}

// offsetMinutes is minutes east of UTC, and 0 with no offset. XSD 1.1 bounds an offset at 14:00.
func offsetMinutes(tz, what, raw string) (int, error) {
	if tz == "" || tz == "Z" {
		return 0, nil
	}
	hours, minutes := twoDigits(tz[1:3]), twoDigits(tz[4:6])
	if minutes > 59 || hours > 14 || (hours == 14 && minutes > 0) {
		return 0, fmt.Errorf("%s: %q has offset %s; an offset is at most 14:00 either way", what, raw, tz)
	}
	if tz[0] == '-' {
		return -(hours*60 + minutes), nil
	}
	return hours*60 + minutes, nil
}

// clockOf applies XSD 1.1: no 60th second, and 24:00:00 only as the first instant of the next day.
func clockOf(hh, mi, ss, fraction, what, raw string) (int, int, error) {
	hour, minute, second := twoDigits(hh), twoDigits(mi), twoDigits(ss)
	endOfDay := hour == 24 && minute == 0 && second == 0 && fraction == ""
	if minute > 59 || second > 59 || (hour > 23 && !endOfDay) {
		return 0, 0, fmt.Errorf("%s: %q has time %s:%s:%s%s; XSD 1.1 allows 00:00:00 to 23:59:59, and 24:00:00 exactly", what, raw, hh, mi, ss, fraction)
	}
	return hour, minute, nil
}

func canonicalFraction(frac string) string {
	if frac == "" {
		return ""
	}
	t := strings.TrimRight(strings.TrimPrefix(frac, "."), "0")
	if t == "" {
		return ""
	}
	return "." + t
}

func canonicalTz(tz string) string {
	if tz == "" {
		return ""
	}
	if tz == "Z" || tz == "+00:00" || tz == "-00:00" {
		return "Z"
	}
	return tz
}

func CanonicalDateTime(raw string) (string, error) {
	const what = "canonicalDateTime"
	m := dateTimeRe.FindStringSubmatch(strings.TrimSpace(raw))
	if m == nil {
		return "", fmt.Errorf("%s: %q invalid", what, raw)
	}
	fraction := canonicalFraction(m[7])
	tz := m[8]
	c, err := civilOf(m[1], m[2], m[3], what, raw)
	if err != nil {
		return "", err
	}
	hour, minute, err := clockOf(m[4], m[5], m[6], fraction, what, raw)
	if err != nil {
		return "", err
	}
	offset, err := offsetMinutes(tz, what, raw)
	if err != nil {
		return "", err
	}
	minutes := hour*60 + minute - offset
	if minutes < 0 {
		minutes += 1440
		c = previousDay(c)
	} else if minutes >= 1440 {
		minutes -= 1440
		c = nextDay(c)
	}
	zone := ""
	if tz != "" {
		zone = "Z"
	}
	return fmt.Sprintf("%s-%s-%sT%s:%s:%s%s%s", yearText(c.year), pad2(c.month), pad2(c.day),
		pad2(minutes/60), pad2(minutes%60), m[6], fraction, zone), nil
}

func CanonicalDate(raw string) (string, error) {
	const what = "canonicalDate"
	m := dateRe.FindStringSubmatch(strings.TrimSpace(raw))
	if m == nil {
		return "", fmt.Errorf("%s: %q invalid", what, raw)
	}
	c, err := civilOf(m[1], m[2], m[3], what, raw)
	if err != nil {
		return "", err
	}
	if _, err := offsetMinutes(m[4], what, raw); err != nil {
		return "", err
	}
	return fmt.Sprintf("%s-%s-%s%s", yearText(c.year), m[2], m[3], canonicalTz(m[4])), nil
}

func CanonicalTime(raw string) (string, error) {
	const what = "canonicalTime"
	m := timeRe.FindStringSubmatch(strings.TrimSpace(raw))
	if m == nil {
		return "", fmt.Errorf("%s: %q invalid", what, raw)
	}
	fraction := canonicalFraction(m[4])
	hour, _, err := clockOf(m[1], m[2], m[3], fraction, what, raw)
	if err != nil {
		return "", err
	}
	if _, err := offsetMinutes(m[5], what, raw); err != nil {
		return "", err
	}
	hh := m[1]
	if hour == 24 {
		hh = "00"
	}
	return fmt.Sprintf("%s:%s:%s%s%s", hh, m[2], m[3], fraction, canonicalTz(m[5])), nil
}

func pad2(n int) string {
	if n < 10 {
		return "0" + strconv.Itoa(n)
	}
	return strconv.Itoa(n)
}

func CanonicalScalarLexical(c Carrier, raw string) (string, error) {
	switch c {
	case CarrierInteger:
		return CanonicalInteger(raw)
	case CarrierDecimal:
		return CanonicalDecimal(raw)
	case CarrierDouble:
		return CanonicalDouble(raw)
	case CarrierFloat:
		return CanonicalFloat(raw)
	case CarrierBoolean:
		return CanonicalBoolean(raw)
	case CarrierString, CarrierAnyURI:
		return CanonicalString(raw), nil
	case CarrierLangString:
		return "", fmt.Errorf("canonicalLangString: a language string is a structured value, an embedded { lexicalForm, language }, so its tag is hashed with it; the langString carrier has no slot for the tag and refuses the scalar %q", raw)
	case CarrierDateTime:
		return CanonicalDateTime(raw)
	case CarrierDate:
		return CanonicalDate(raw)
	case CarrierTime:
		return CanonicalTime(raw)
	case CarrierHexBinary:
		return CanonicalHexBinary(raw)
	case CarrierBase64Binary:
		return CanonicalBase64(raw)
	}
	return "", fmt.Errorf("unknown carrier %q", c)
}

// ---------------------------------------------------------------------------
// Value model + wire form
// ---------------------------------------------------------------------------

type Value interface{ isValue() }

type Typed struct {
	Carrier Carrier
	Lexical string
}
type Raw struct{ Token string }
type Ref struct{ URI string }
type Embedded struct {
	Name       *string
	Statements []Statement
}
type List struct{ Items []Value }

func (Typed) isValue()    {}
func (Raw) isValue()      {}
func (Ref) isValue()      {}
func (Embedded) isValue() {}
func (List) isValue()     {}

type Statement struct {
	Predicate string
	Value     Value
}
type Subject struct {
	URI        string
	Statements []Statement
}
type Package struct{ Subjects []Subject }

func CanonicalForm(pkg Package) (string, error) {
	var b strings.Builder
	b.WriteString(`{"subjects":[`)
	subs := append([]Subject(nil), pkg.Subjects...)
	sort.SliceStable(subs, func(i, j int) bool { return subs[i].URI < subs[j].URI })
	for i, s := range subs {
		if i > 0 {
			b.WriteByte(',')
		}
		b.WriteString(`{"uri":`)
		emitJSONString(&b, s.URI)
		b.WriteString(`,"statements":[`)
		if err := emitStatements(&b, s.Statements); err != nil {
			return "", err
		}
		b.WriteString("]}")
	}
	b.WriteString("]}")
	return b.String(), nil
}

func CanonicalHash(pkg Package) (string, error) {
	form, err := CanonicalForm(pkg)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256([]byte(form))
	return "sha256:" + hex.EncodeToString(sum[:]), nil
}

func serializeStatement(st Statement) (string, error) {
	var b strings.Builder
	b.WriteString(`{"predicate":`)
	emitJSONString(&b, st.Predicate)
	b.WriteByte(',')
	if err := emitValueTail(&b, st.Value); err != nil {
		return "", err
	}
	b.WriteByte('}')
	return b.String(), nil
}

// emitStatements orders by predicate UTF-8 bytes (Go string comparison IS
// byte-wise lexicographic); equal predicates (possible since multi-typed
// subjects — several type statements share the type predicate) order by the
// serialized statement blob's UTF-8 bytes. The tie-break makes the declared
// invariance under statement ordering TRUE for same-predicate statements
// rather than an accident of sort stability; no distinct-predicate ordering
// is affected.
func emitStatements(b *strings.Builder, stmts []Statement) error {
	type rendered struct {
		predicate  string
		serialized string
	}
	ordered := make([]rendered, 0, len(stmts))
	for _, st := range stmts {
		serialized, err := serializeStatement(st)
		if err != nil {
			return err
		}
		ordered = append(ordered, rendered{predicate: st.Predicate, serialized: serialized})
	}
	sort.Slice(ordered, func(i, j int) bool {
		if ordered[i].predicate != ordered[j].predicate {
			return ordered[i].predicate < ordered[j].predicate
		}
		return ordered[i].serialized < ordered[j].serialized
	})
	for i, st := range ordered {
		if i > 0 {
			b.WriteByte(',')
		}
		b.WriteString(st.serialized)
	}
	return nil
}

func emitValueTail(b *strings.Builder, v Value) error {
	switch x := v.(type) {
	case Typed:
		lex, err := CanonicalScalarLexical(x.Carrier, x.Lexical)
		if err != nil {
			return err
		}
		b.WriteString(`"type":"typed","carrier":`)
		emitJSONString(b, string(x.Carrier))
		b.WriteString(`,"value":`)
		emitJSONString(b, lex)
	case Raw:
		b.WriteString(`"type":"string","value":`)
		emitJSONString(b, x.Token)
	case Ref:
		b.WriteString(`"type":"ref","value":`)
		emitJSONString(b, x.URI)
	case Embedded:
		b.WriteString(`"type":"embedded"`)
		if x.Name != nil {
			b.WriteString(`,"name":`)
			emitJSONString(b, *x.Name)
		}
		b.WriteString(`,"statements":[`)
		if err := emitStatements(b, x.Statements); err != nil {
			return err
		}
		b.WriteByte(']')
	case List:
		b.WriteString(`"type":"list","items":[`)
		for i, item := range x.Items {
			if i > 0 {
				b.WriteByte(',')
			}
			b.WriteByte('{')
			if err := emitValueTail(b, item); err != nil {
				return err
			}
			b.WriteByte('}')
		}
		b.WriteByte(']')
	default:
		return fmt.Errorf("canonicalForm: unrecognized value kind %T", v)
	}
	return nil
}

// emitJSONString writes RFC 8785 / JSON.stringify escaping.
func emitJSONString(b *strings.Builder, s string) {
	b.WriteByte('"')
	for _, c := range s {
		switch c {
		case '"':
			b.WriteString(`\"`)
		case '\\':
			b.WriteString(`\\`)
		case '\b':
			b.WriteString(`\b`)
		case '\f':
			b.WriteString(`\f`)
		case '\n':
			b.WriteString(`\n`)
		case '\r':
			b.WriteString(`\r`)
		case '\t':
			b.WriteString(`\t`)
		default:
			if c < 0x20 {
				fmt.Fprintf(b, `\u%04x`, c)
			} else {
				b.WriteRune(c)
			}
		}
	}
	b.WriteByte('"')
}
