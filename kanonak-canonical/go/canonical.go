// Package canonical computes the Kanonak canonical form + content hash
// (canonicalFormVersion "1"). An independent conformant port of
// kanonak.org/canonical-form, verified byte-for-byte against the golden vectors.
package canonical

import (
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"fmt"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"time"

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

// carrierKey returns publisher/package/name from a datatype URI.
func carrierKey(uri string) string {
	idx := strings.LastIndex(uri, "/")
	name := uri[idx+1:]
	head := uri[:idx]
	slash := strings.Index(head, "/")
	publisher := head[:slash]
	pkg := head[slash+1:]
	if at := strings.Index(pkg, "@"); at >= 0 {
		pkg = pkg[:at]
	}
	return publisher + "/" + pkg + "/" + name
}

// CarrierOf returns the carrier for a datatype URI, or ok=false (out-of-set).
func CarrierOf(uri string) (Carrier, bool) {
	key := carrierKey(uri)
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
	if single {
		f := float64(float32(n))
		if f == 0 {
			return "0", nil
		}
		return strconv.FormatFloat(f, 'g', -1, 32), nil
	}
	if n == 0 {
		return "0", nil
	}
	return strconv.FormatFloat(n, 'g', -1, 64), nil
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

// -- temporal ---------------------------------------------------------------

func canonicalYear(raw string) string {
	neg := strings.HasPrefix(raw, "-")
	d := raw
	if neg {
		d = raw[1:]
	}
	d = strings.TrimLeft(d, "0")
	if d == "" {
		d = "0"
	}
	if len(d) < 4 {
		d = strings.Repeat("0", 4-len(d)) + d
	}
	if neg {
		return "-" + d
	}
	return d
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

func tzOffsetMinutes(tz string) int {
	if tz == "Z" {
		return 0
	}
	sign := 1
	if tz[0] == '-' {
		sign = -1
	}
	hh, _ := strconv.Atoi(tz[1:3])
	mm, _ := strconv.Atoi(tz[4:6])
	return sign * (hh*60 + mm)
}

func atoi(s string) int { n, _ := strconv.Atoi(s); return n }

func CanonicalDateTime(raw string) (string, error) {
	m := dateTimeRe.FindStringSubmatch(strings.TrimSpace(raw))
	if m == nil {
		return "", fmt.Errorf("canonicalDateTime: %q invalid", raw)
	}
	fraction := canonicalFraction(m[7])
	tz := m[8]
	if tz == "" {
		return fmt.Sprintf("%s-%s-%sT%s:%s:%s%s", canonicalYear(m[1]), m[2], m[3], m[4], m[5], m[6], fraction), nil
	}
	t := time.Date(atoi(m[1]), time.Month(atoi(m[2])), atoi(m[3]), atoi(m[4]), atoi(m[5]), atoi(m[6]), 0, time.UTC)
	t = t.Add(-time.Duration(tzOffsetMinutes(tz)) * time.Minute)
	return fmt.Sprintf("%s-%s-%sT%s:%s:%s%sZ",
		canonicalYear(strconv.Itoa(t.Year())), pad2(int(t.Month())), pad2(t.Day()),
		pad2(t.Hour()), pad2(t.Minute()), pad2(t.Second()), fraction), nil
}

func CanonicalDate(raw string) (string, error) {
	m := dateRe.FindStringSubmatch(strings.TrimSpace(raw))
	if m == nil {
		return "", fmt.Errorf("canonicalDate: %q invalid", raw)
	}
	return fmt.Sprintf("%s-%s-%s%s", canonicalYear(m[1]), m[2], m[3], canonicalTz(m[4])), nil
}

func CanonicalTime(raw string) (string, error) {
	m := timeRe.FindStringSubmatch(strings.TrimSpace(raw))
	if m == nil {
		return "", fmt.Errorf("canonicalTime: %q invalid", raw)
	}
	hh := m[1]
	fraction := canonicalFraction(m[4])
	if hh == "24" && m[2] == "00" && m[3] == "00" && fraction == "" {
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
	case CarrierString, CarrierAnyURI, CarrierLangString:
		return CanonicalString(raw), nil
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

func emitStatements(b *strings.Builder, stmts []Statement) error {
	ordered := append([]Statement(nil), stmts...)
	sort.SliceStable(ordered, func(i, j int) bool { return ordered[i].Predicate < ordered[j].Predicate })
	for i, st := range ordered {
		if i > 0 {
			b.WriteByte(',')
		}
		b.WriteString(`{"predicate":`)
		emitJSONString(b, st.Predicate)
		b.WriteByte(',')
		if err := emitValueTail(b, st.Value); err != nil {
			return err
		}
		b.WriteByte('}')
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
