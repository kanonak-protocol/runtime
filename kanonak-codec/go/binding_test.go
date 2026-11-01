package codec

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
	"reflect"
	"strconv"
	"strings"
	"testing"
)

// The encoding/json binding of the JSON forms (runtime#54): each read and
// write case of the shared vectors through encoding/json, with each field typed
// as a generated struct types it. Our types refuse with [json-number-form];
// int32 and the rest refuse with encoding/json's own error, which the forms ask
// for.

// amount is the embedded Amount, as a generated struct holds it.
type amount struct {
	Value Decimal `json:"value"`
}

type bindingCase struct {
	ID          string      `json:"id"`
	Field       string      `json:"field"`
	JSON        *string     `json:"json"`
	Expected    interface{} `json:"expected"`
	ExpectError bool        `json:"expectError"`
	Value       interface{} `json:"value"`
}

type bindingDoc struct {
	ReadCases  []bindingCase `json:"readCases"`
	WriteCases []bindingCase `json:"writeCases"`
}

// boundLexical is what a read value reads as, compared with a case's expected:
// each value's canonical lexical form.
func boundLexical(v interface{}) interface{} {
	switch x := v.(type) {
	case fmt.Stringer:
		return x.String()
	case int32, int16, int8, uint32, uint16, uint8:
		return fmt.Sprint(x)
	case []Integer[XsdInteger]:
		out := make([]interface{}, len(x))
		for i, n := range x {
			out[i] = n.String()
		}
		return out
	case amount:
		return map[string]interface{}{"value": x.Value.String()}
	}
	panic(fmt.Sprintf("no lexical form for %T", v))
}

// readCase reads a case's JSON into a new value of the field's type.
func readCase(field, text string) (interface{}, bool, error) {
	read := func(target interface{}, ours bool) (interface{}, bool, error) {
		err := json.Unmarshal([]byte(text), target)
		return reflect.ValueOf(target).Elem().Interface(), ours, err
	}
	switch field {
	case "integer":
		return read(new(Integer[XsdInteger]), true)
	case "nonNegativeInteger":
		return read(new(Integer[NonNegativeInteger]), true)
	case "positiveInteger":
		return read(new(Integer[PositiveInteger]), true)
	case "nonPositiveInteger":
		return read(new(Integer[NonPositiveInteger]), true)
	case "negativeInteger":
		return read(new(Integer[NegativeInteger]), true)
	case "long":
		return read(new(Long), true)
	case "unsignedLong":
		return read(new(UnsignedLong), true)
	case "decimal":
		return read(new(Decimal), true)
	case "int":
		return read(new(int32), false)
	case "short":
		return read(new(int16), false)
	case "byte":
		return read(new(int8), false)
	case "unsignedInt":
		return read(new(uint32), false)
	case "unsignedShort":
		return read(new(uint16), false)
	case "unsignedByte":
		return read(new(uint8), false)
	case "counts":
		return read(new([]Integer[XsdInteger]), true)
	case "amount":
		return read(new(amount), true)
	}
	return nil, false, fmt.Errorf("no binding type for field %s", field)
}

func narrowOf(v interface{}, bits int, unsigned bool) (interface{}, error) {
	s := v.(string)
	if unsigned {
		n, err := strconv.ParseUint(s, 10, bits)
		return n, err
	}
	n, err := strconv.ParseInt(s, 10, bits)
	return n, err
}

// heldCase is the value a write case holds, made from its canonical lexical
// form, or the error when the field's type can't hold it.
func heldCase(field string, v interface{}) (interface{}, error) {
	text := func() string { return v.(string) }
	switch field {
	case "integer":
		return ParseInteger[XsdInteger](text())
	case "nonNegativeInteger":
		return ParseInteger[NonNegativeInteger](text())
	case "positiveInteger":
		return ParseInteger[PositiveInteger](text())
	case "long":
		return ParseLong(text())
	case "unsignedLong":
		return ParseUnsignedLong(text())
	case "decimal":
		return ParseDecimal(text())
	case "int":
		n, err := narrowOf(v, 32, false)
		if err != nil {
			return nil, err
		}
		return int32(n.(int64)), nil
	case "byte":
		n, err := narrowOf(v, 8, false)
		if err != nil {
			return nil, err
		}
		return int8(n.(int64)), nil
	case "unsignedShort":
		n, err := narrowOf(v, 16, true)
		if err != nil {
			return nil, err
		}
		return uint16(n.(uint64)), nil
	case "counts":
		var out []Integer[XsdInteger]
		for _, item := range v.([]interface{}) {
			n, err := ParseInteger[XsdInteger](item.(string))
			if err != nil {
				return nil, err
			}
			out = append(out, n)
		}
		return out, nil
	case "amount":
		d, err := ParseDecimal(v.(map[string]interface{})["value"].(string))
		if err != nil {
			return nil, err
		}
		return amount{Value: d}, nil
	}
	return nil, fmt.Errorf("no binding type for field %s", field)
}

func sameJSON(a, b []byte) bool {
	var x, y interface{}
	return json.Unmarshal(a, &x) == nil && json.Unmarshal(b, &y) == nil && reflect.DeepEqual(x, y)
}

func TestJSONNumberBindingVectors(t *testing.T) {
	raw, err := os.ReadFile("../vectors/codec-vectors-json-numbers.json")
	if err != nil {
		t.Fatal(err)
	}
	var doc bindingDoc
	if err := json.Unmarshal(raw, &doc); err != nil {
		t.Fatal(err)
	}
	checked, failed := 0, 0
	fail := func(format string, args ...interface{}) {
		failed++
		t.Errorf(format, args...)
	}
	for _, c := range doc.ReadCases {
		checked++
		got, ours, err := readCase(c.Field, *c.JSON)
		switch {
		case c.ExpectError && err == nil:
			fail("read %s: expected a refusal, got %v", c.ID, boundLexical(got))
		case c.ExpectError && ours && !strings.Contains(err.Error(), "[json-number-form]"):
			fail("read %s: refused without [json-number-form]: %v", c.ID, err)
		case !c.ExpectError && err != nil:
			fail("read %s: refused: %v", c.ID, err)
		case !c.ExpectError && !reflect.DeepEqual(boundLexical(got), c.Expected):
			fail("read %s: expected %v, got %v", c.ID, c.Expected, boundLexical(got))
		}
	}
	for _, c := range doc.WriteCases {
		checked++
		held, err := heldCase(c.Field, c.Value)
		switch {
		case c.JSON == nil && err == nil:
			written, _ := json.Marshal(held)
			fail("write %s: expected a refusal, wrote %s", c.ID, written)
		case c.JSON == nil:
		case err != nil:
			fail("write %s: refused: %v", c.ID, err)
		default:
			written, err := json.Marshal(held)
			if err != nil {
				fail("write %s: %v", c.ID, err)
			} else if !sameJSON(written, []byte(*c.JSON)) {
				fail("write %s: expected %s, got %s", c.ID, *c.JSON, written)
			}
		}
	}
	t.Logf("codec-vectors-json-numbers.json (binding): %d/%d pass", checked-failed, checked)
}

func TestBindingEdges(t *testing.T) {
	// encoding/json reads 9007199254740993 as its token, and the form refuses it past ±(2^53 − 1).
	var i Integer[XsdInteger]
	if err := json.Unmarshal([]byte("9007199254740993"), &i); err == nil {
		t.Error("an integer number past 2^53 − 1 was read")
	}
	var l Long
	if err := json.Unmarshal([]byte(`"9223372036854775807"`), &l); err != nil || l != Long(math.MaxInt64) {
		t.Errorf("the widest long didn't read from its string: %v %v", l, err)
	}
	// The zero value of a positive integer isn't in its range, so it isn't written.
	if _, err := json.Marshal(Integer[PositiveInteger]{}); err == nil {
		t.Error("a zero positiveInteger was written")
	}
	if b, err := json.Marshal(Integer[NonNegativeInteger]{}); err != nil || string(b) != `"0"` {
		t.Errorf("a zero nonNegativeInteger wrote %s, %v", b, err)
	}
	// A null leaves a field as it is, as encoding/json's own types do.
	held := Long(5)
	if err := json.Unmarshal([]byte("null"), &held); err != nil || held != 5 {
		t.Errorf("null changed a Long: %v %v", held, err)
	}
	// The case that needed it: core-owl's cardinality, an xsd:nonNegativeInteger, in the form "1".
	var restriction struct {
		Cardinality Integer[NonNegativeInteger] `json:"cardinality"`
	}
	if err := json.Unmarshal([]byte(`{"cardinality": "1"}`), &restriction); err != nil || restriction.Cardinality.String() != "1" {
		t.Errorf("cardinality \"1\" didn't read: %v %v", restriction.Cardinality, err)
	}
}
