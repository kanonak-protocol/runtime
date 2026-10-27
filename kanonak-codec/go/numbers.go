// The JSON forms of integer and decimal values (kanonak-protocol 4.11.0's
// numbers-in-json, runtime#54). A JSON number can't carry every integer or
// decimal exactly in every language, so a wide integer and a decimal travel as
// a string of their digits, and a reader refuses a number that may already
// have been rounded.

package codec

import (
	"encoding/json"
	"fmt"
	"maps"
	"math"
	"math/big"

	canonical "github.com/kanonak-protocol/runtime/kanonak-canonical/go"
)

type numberForm int

const (
	// wideInteger is written as a string of its canonical digits, and read
	// from a string or a whole JSON number within ±(2^53 − 1).
	wideInteger numberForm = iota
	// decimalNumber is written as a string of its canonical digits, and read
	// from a string only.
	decimalNumber
	// narrowInteger is written as a JSON number, and read from a whole one only.
	narrowInteger
)

// jsonNumberForm is how JSON carries a datatype's values, the range they lie
// in (a nil bound is unbounded), and the Go value DeserializeExact gives an
// integer of it.
type jsonNumberForm struct {
	form     numberForm
	min, max *big.Int
	goValue  func(*big.Int) interface{}
}

const xsd = "kanonak.org/core-xsd/"

var maxSafeInteger = big.NewInt(1<<53 - 1)

func bigIntValue(n *big.Int) interface{} { return n }
func int64Value(n *big.Int) interface{}  { return n.Int64() }
func uint64Value(n *big.Int) interface{} { return n.Uint64() }

var jsonNumberForms = map[string]jsonNumberForm{
	xsd + "integer":            {form: wideInteger, goValue: bigIntValue},
	xsd + "nonNegativeInteger": {form: wideInteger, min: big.NewInt(0), goValue: bigIntValue},
	xsd + "positiveInteger":    {form: wideInteger, min: big.NewInt(1), goValue: bigIntValue},
	xsd + "nonPositiveInteger": {form: wideInteger, max: big.NewInt(0), goValue: bigIntValue},
	xsd + "negativeInteger":    {form: wideInteger, max: big.NewInt(-1), goValue: bigIntValue},
	xsd + "long":               {form: wideInteger, min: big.NewInt(math.MinInt64), max: big.NewInt(math.MaxInt64), goValue: int64Value},
	xsd + "unsignedLong":       {form: wideInteger, min: big.NewInt(0), max: new(big.Int).SetUint64(math.MaxUint64), goValue: uint64Value},
	xsd + "decimal":            {form: decimalNumber},
	xsd + "int":                {form: narrowInteger, min: big.NewInt(math.MinInt32), max: big.NewInt(math.MaxInt32), goValue: int64Value},
	xsd + "short":              {form: narrowInteger, min: big.NewInt(math.MinInt16), max: big.NewInt(math.MaxInt16), goValue: int64Value},
	xsd + "byte":               {form: narrowInteger, min: big.NewInt(math.MinInt8), max: big.NewInt(math.MaxInt8), goValue: int64Value},
	xsd + "unsignedInt":        {form: narrowInteger, min: big.NewInt(0), max: big.NewInt(math.MaxUint32), goValue: int64Value},
	xsd + "unsignedShort":      {form: narrowInteger, min: big.NewInt(0), max: big.NewInt(math.MaxUint16), goValue: int64Value},
	xsd + "unsignedByte":       {form: narrowInteger, min: big.NewInt(0), max: big.NewInt(math.MaxUint8), goValue: int64Value},
}

type numberConversion func(prop Prop, form jsonNumberForm, value interface{}) (interface{}, error)

func jsonNumberError(prop Prop, why string) error {
	return fmt.Errorf("codec: %s: %s [json-number-form]", prop.Predicate, why)
}

func describeJSON(value interface{}) string {
	if s, ok := value.(string); ok {
		return fmt.Sprintf("the string %q", s)
	}
	return fmt.Sprintf("%T %v", value, value)
}

func inRange(prop Prop, form jsonNumberForm, n *big.Int) error {
	if (form.min != nil && n.Cmp(form.min) < 0) || (form.max != nil && n.Cmp(form.max) > 0) {
		return jsonNumberError(prop, fmt.Sprintf("%s is outside %s's range", n, prop.Datatype))
	}
	return nil
}

func integerOfLexical(prop Prop, lexical string) (*big.Int, error) {
	if canon, err := canonical.CanonicalInteger(lexical); err == nil {
		if n, ok := new(big.Int).SetString(canon, 10); ok {
			return n, nil
		}
	}
	return nil, jsonNumberError(prop, fmt.Sprintf("%q is not an integer lexical form", lexical))
}

func decimalOfLexical(prop Prop, lexical string) (string, error) {
	canon, err := canonical.CanonicalDecimal(lexical)
	if err != nil {
		return "", jsonNumberError(prop, fmt.Sprintf("%q is not a decimal lexical form", lexical))
	}
	return canon, nil
}

func goInteger(value interface{}) (*big.Int, bool) {
	switch v := value.(type) {
	case int:
		return big.NewInt(int64(v)), true
	case int8:
		return big.NewInt(int64(v)), true
	case int16:
		return big.NewInt(int64(v)), true
	case int32:
		return big.NewInt(int64(v)), true
	case int64:
		return big.NewInt(v), true
	case uint:
		return new(big.Int).SetUint64(uint64(v)), true
	case uint8:
		return new(big.Int).SetUint64(uint64(v)), true
	case uint16:
		return new(big.Int).SetUint64(uint64(v)), true
	case uint32:
		return new(big.Int).SetUint64(uint64(v)), true
	case uint64:
		return new(big.Int).SetUint64(v), true
	}
	return nil, false
}

// wholeNumber is the value of a JSON number, decoded as a json.Number or a
// float64 or held as a Go integer, when that value is a whole number. A
// json.Number is read at its exact value, since a float64 would take
// 1.0000000000000000001 for 1; strconv first refuses a token that isn't a
// number, which big.Rat would read as one, such as 4/2.
func wholeNumber(value interface{}) (*big.Int, bool) {
	if n, ok := goInteger(value); ok {
		return n, true
	}
	switch v := value.(type) {
	case json.Number:
		if _, err := v.Float64(); err != nil {
			return nil, false
		}
		r, ok := new(big.Rat).SetString(v.String())
		if !ok || !r.IsInt() {
			return nil, false
		}
		return r.Num(), true
	case float64:
		if math.IsInf(v, 0) || v != math.Trunc(v) {
			return nil, false
		}
		n, _ := big.NewFloat(v).Int(nil)
		return n, true
	}
	return nil, false
}

// readJSONNumber is a value as JSON carries it, to its exact Go value: a wide
// integer a *big.Int, an int64 or a uint64, a decimal its canonical digits, a
// narrow integer an int64.
func readJSONNumber(prop Prop, form jsonNumberForm, value interface{}) (interface{}, error) {
	switch form.form {
	case decimalNumber:
		s, ok := value.(string)
		if !ok {
			return nil, jsonNumberError(prop,
				"a decimal is read from a JSON string only, so it can't have been rounded; got "+describeJSON(value))
		}
		return decimalOfLexical(prop, s)
	case wideInteger:
		var n *big.Int
		if s, ok := value.(string); ok {
			var err error
			if n, err = integerOfLexical(prop, s); err != nil {
				return nil, err
			}
		} else if whole, ok := wholeNumber(value); ok && whole.CmpAbs(maxSafeInteger) <= 0 {
			n = whole
		} else {
			return nil, jsonNumberError(prop,
				"a wide integer is read from a string, or from a whole JSON number within ±(2^53 − 1); got "+
					describeJSON(value))
		}
		if err := inRange(prop, form, n); err != nil {
			return nil, err
		}
		return form.goValue(n), nil
	default:
		n, ok := wholeNumber(value)
		if !ok {
			return nil, jsonNumberError(prop, "a narrow integer is read from a whole JSON number only; got "+describeJSON(value))
		}
		if err := inRange(prop, form, n); err != nil {
			return nil, err
		}
		return form.goValue(n), nil
	}
}

func wideIntegerToWrite(prop Prop, value interface{}) (*big.Int, error) {
	switch v := value.(type) {
	case *big.Int:
		if v != nil {
			return v, nil
		}
	case big.Int:
		return &v, nil
	case string:
		return integerOfLexical(prop, v)
	}
	if n, ok := goInteger(value); ok {
		return n, nil
	}
	if n, ok := wholeNumber(value); ok && n.CmpAbs(maxSafeInteger) <= 0 {
		return n, nil
	}
	return nil, jsonNumberError(prop,
		"pass a wide integer as a *big.Int, a Go integer, a string, or a whole number within ±(2^53 − 1); got "+
			describeJSON(value))
}

// writeJSONNumber is a value of its exact Go type, to its JSON form: a wide
// integer and a decimal as a string of canonical digits, a narrow integer as
// an int64.
func writeJSONNumber(prop Prop, form jsonNumberForm, value interface{}) (interface{}, error) {
	switch form.form {
	case decimalNumber:
		s, ok := value.(string)
		if !ok {
			return nil, jsonNumberError(prop,
				"pass a decimal as a string of its digits, which holds it exactly; got "+describeJSON(value))
		}
		return decimalOfLexical(prop, s)
	case wideInteger:
		n, err := wideIntegerToWrite(prop, value)
		if err != nil {
			return nil, err
		}
		if err := inRange(prop, form, n); err != nil {
			return nil, err
		}
		return n.String(), nil
	default:
		n, ok := wholeNumber(value)
		if !ok {
			return nil, jsonNumberError(prop, "pass a narrow integer as a Go integer or a whole number; got "+describeJSON(value))
		}
		if err := inRange(prop, form, n); err != nil {
			return nil, err
		}
		return n.Int64(), nil
	}
}

func jsonNumberFormOf(prop Prop) (jsonNumberForm, bool) {
	if prop.Kind != "datatype" || prop.Datatype == "" {
		return jsonNumberForm{}, false
	}
	key, ok := canonical.LenientVersionlessKey(prop.Datatype)
	if !ok {
		return jsonNumberForm{}, false
	}
	form, ok := jsonNumberForms[key]
	return form, ok
}

// convertFields is a copy of fields with each modeled field's value converted,
// through lists and embedded values.
func convertFields(fields map[string]interface{}, cls Class, schema CodecSchema, convert numberConversion) (map[string]interface{}, error) {
	out := maps.Clone(fields)
	for name, prop := range cls.Props {
		val := out[name]
		if val == nil {
			continue
		}
		converted, err := convertValue(prop, val, schema, convert)
		if err != nil {
			return nil, err
		}
		out[name] = converted
	}
	return out, nil
}

func convertValue(prop Prop, value interface{}, schema CodecSchema, convert numberConversion) (interface{}, error) {
	if list, isList := value.([]interface{}); isList {
		out := make([]interface{}, len(list))
		for i, item := range list {
			converted, err := convertValue(prop, item, schema, convert)
			if err != nil {
				return nil, err
			}
			out[i] = converted
		}
		return out, nil
	}
	if prop.Kind == "datatype" {
		if form, ok := jsonNumberFormOf(prop); ok {
			return convert(prop, form, value)
		}
		return value, nil
	}
	m, isMap := value.(map[string]interface{})
	if !isMap {
		return value, nil
	}
	if _, isRef := m["$ref"]; isRef {
		return value, nil
	}
	clsURI, _ := m["$type"].(string)
	if clsURI == "" {
		clsURI = prop.Range
	}
	if clsURI == "" {
		return nil, fmt.Errorf(
			"codec: cannot convert the embedded value under %s: it carries no $type and the "+
				"property declares no range", prop.Predicate)
	}
	cls, err := classFor(schema, clsURI)
	if err != nil {
		return nil, fmt.Errorf("codec: cannot convert the embedded value under %s: %w", prop.Predicate, err)
	}
	return convertFields(m, cls, schema, convert)
}

// SerializeExact renders a node to normalized JSON as Serialize does, writing
// each integer and decimal value in its JSON form (runtime#54): a wide integer
// and a decimal as a string of their canonical digits, a narrow integer as an
// int64. A wide integer is taken as a *big.Int, a big.Int, a Go integer, a
// lexical string or a whole number within ±(2^53 − 1); a decimal as a string
// only; a narrow integer as a Go integer or a whole number. The node's $type
// must name a class in the schema. A value outside its datatype's range, or of
// another type, is refused, ending [json-number-form].
func SerializeExact(node map[string]interface{}, schema CodecSchema) (map[string]interface{}, error) {
	typeURI, ok := node["$type"].(string)
	if !ok {
		return nil, fmt.Errorf("codec: cannot serialize: missing string $type")
	}
	cls, err := classFor(schema, typeURI)
	if err != nil {
		return nil, fmt.Errorf("codec: cannot serialize: %w", err)
	}
	converted, err := convertFields(node, cls, schema, writeJSONNumber)
	if err != nil {
		return nil, err
	}
	return Serialize(converted)
}

// DeserializeExact parses normalized JSON into a node as Deserialize does,
// reading each integer and decimal value exactly (runtime#54): an integer,
// nonNegativeInteger, positiveInteger, nonPositiveInteger or negativeInteger
// to a *big.Int, a long to an int64 and an unsignedLong to a uint64, each from
// a string or a whole number within ±(2^53 − 1); a decimal to a string of its
// canonical digits, from a string only; a narrow integer to an int64, from a
// whole number only. A number may arrive as a json.Number or a float64. A
// value JSON may already have rounded, or one outside its datatype's range, is
// refused, ending [json-number-form].
func DeserializeExact(jsonObj map[string]interface{}, schema CodecSchema) (map[string]interface{}, error) {
	node, err := Deserialize(jsonObj, schema)
	if err != nil {
		return nil, err
	}
	typeURI, _ := node["$type"].(string)
	cls, err := classFor(schema, typeURI)
	if err != nil {
		return nil, fmt.Errorf("codec: cannot deserialize: %w", err)
	}
	return convertFields(node, cls, schema, readJSONNumber)
}
