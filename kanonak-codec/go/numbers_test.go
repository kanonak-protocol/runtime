package codec

import (
	"encoding/json"
	"fmt"
	"maps"
	"math/big"
	"os"
	"reflect"
	"strconv"
	"strings"
	"testing"

	canonical "github.com/kanonak-protocol/runtime/kanonak-canonical/go"
)

type jsonNumbersVectors struct {
	Class     string      `json:"class"`
	Schema    CodecSchema `json:"schema"`
	ReadCases []struct {
		ID          string      `json:"id"`
		Field       string      `json:"field"`
		JSON        string      `json:"json"`
		Expected    interface{} `json:"expected"`
		ExpectError bool        `json:"expectError"`
	} `json:"readCases"`
	WriteCases []struct {
		ID          string      `json:"id"`
		Field       string      `json:"field"`
		Value       interface{} `json:"value"`
		JSON        string      `json:"json"`
		ExpectError bool        `json:"expectError"`
	} `json:"writeCases"`
}

// exactTypes is the Go type Deserialize gives a value of each datatype,
// and Serialize takes (Paul's proxy, 2026-10-27).
var exactTypes = map[string]string{
	"integer": "*big.Int", "nonNegativeInteger": "*big.Int", "positiveInteger": "*big.Int",
	"nonPositiveInteger": "*big.Int", "negativeInteger": "*big.Int",
	"long": "int64", "unsignedLong": "uint64", "decimal": "string",
	"int": "int64", "short": "int64", "byte": "int64",
	"unsignedInt": "int64", "unsignedShort": "int64", "unsignedByte": "int64",
}

func exactTypeOf(t *testing.T, prop Prop) string {
	t.Helper()
	name, err := canonical.LocalName(prop.Datatype)
	if err != nil || exactTypes[name] == "" {
		t.Fatalf("%s: no exact type for datatype %q", prop.Predicate, prop.Datatype)
	}
	return exactTypes[name]
}

// lexicalOf renders a value Deserialize gave back to its canonical lexical
// form, item by item and field by field, checking each is of its exact type.
func lexicalOf(t *testing.T, id string, prop Prop, value interface{}, schema CodecSchema) interface{} {
	t.Helper()
	switch v := value.(type) {
	case []interface{}:
		out := make([]interface{}, len(v))
		for i, item := range v {
			out[i] = lexicalOf(t, id, prop, item, schema)
		}
		return out
	case map[string]interface{}:
		cls := schema.Classes[prop.Range]
		out := make(map[string]interface{}, len(v))
		for key, item := range v {
			out[key] = lexicalOf(t, id, cls.Props[key], item, schema)
		}
		return out
	}
	if got, want := fmt.Sprintf("%T", value), exactTypeOf(t, prop); got != want {
		t.Errorf("[%s] read %s as %s, want %s", id, prop.Datatype, got, want)
	}
	return fmt.Sprint(value)
}

// exactValue is the value whose canonical lexical form is lexical, in the Go
// type this port gives prop's datatype, or a *big.Int where that type can't
// hold it.
func exactValue(t *testing.T, prop Prop, lexical interface{}, schema CodecSchema) interface{} {
	t.Helper()
	switch v := lexical.(type) {
	case []interface{}:
		out := make([]interface{}, len(v))
		for i, item := range v {
			out[i] = exactValue(t, prop, item, schema)
		}
		return out
	case map[string]interface{}:
		cls := schema.Classes[prop.Range]
		out := make(map[string]interface{}, len(v))
		for key, item := range v {
			out[key] = exactValue(t, cls.Props[key], item, schema)
		}
		return out
	}
	text := lexical.(string)
	switch exactTypeOf(t, prop) {
	case "string":
		return text
	case "int64":
		if n, err := strconv.ParseInt(text, 10, 64); err == nil {
			return n
		}
	case "uint64":
		if n, err := strconv.ParseUint(text, 10, 64); err == nil {
			return n
		}
	}
	n, ok := new(big.Int).SetString(text, 10)
	if !ok {
		t.Fatalf("%s: %q is not a canonical integer", prop.Predicate, text)
	}
	return n
}

// asJSON is v as JSON reads it back, numbers kept as json.Number, so values
// compare by what they'd put on the wire.
func asJSON(t *testing.T, v interface{}) interface{} {
	t.Helper()
	var out interface{}
	decodeNumberAware(t, mustMarshal(t, v), &out)
	return out
}

// TestCodecVectorsJSONNumbers drives the JSON forms of integer and decimal
// values (runtime#54) through Serialize and Deserialize. Each read
// case runs twice, decoded with UseNumber and without it, since a number
// reaches Deserialize as a json.Number or a float64. A value read back is
// written again, so what Deserialize gives Serialize takes, and hashed, so
// ContentHash takes it too.
func TestCodecVectorsJSONNumbers(t *testing.T) {
	const path = "../vectors/codec-vectors-json-numbers.json"
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read vectors: %v", err)
	}
	var doc jsonNumbersVectors
	decodeNumberAware(t, data, &doc)
	schema := doc.Schema
	cls, ok := schema.Classes[doc.Class]
	if !ok {
		t.Fatalf("vectors' class %s is not in their schema", doc.Class)
	}
	refused := func(err error) bool {
		return err != nil && strings.HasSuffix(err.Error(), "[json-number-form]")
	}

	decoders := map[string]func([]byte) map[string]interface{}{
		"json.Number": func(text []byte) map[string]interface{} {
			var obj map[string]interface{}
			decodeNumberAware(t, text, &obj)
			return obj
		},
		"float64": func(text []byte) map[string]interface{} {
			var obj map[string]interface{}
			if err := json.Unmarshal(text, &obj); err != nil {
				t.Fatalf("decode: %v", err)
			}
			return obj
		},
	}

	total := 0
	for decoding, decode := range decoders {
		for _, c := range doc.ReadCases {
			total++
			id := c.ID + " (" + decoding + ")"
			prop, ok := cls.Props[c.Field]
			if !ok {
				t.Fatalf("[%s] field %s is not modeled on %s", id, c.Field, doc.Class)
			}
			classJSON, _ := json.Marshal(doc.Class)
			fieldJSON, _ := json.Marshal(c.Field)
			text := fmt.Sprintf(`{"$type": %s, %s: %s}`, classJSON, fieldJSON, c.JSON)

			node, err := Deserialize(decode([]byte(text)), schema)
			if c.ExpectError {
				if !refused(err) {
					t.Errorf("[%s] read: expected a [json-number-form] refusal, got %v (err %v)", id, node, err)
				}
				continue
			}
			if err != nil {
				t.Errorf("[%s] read: %v", id, err)
				continue
			}
			if got := lexicalOf(t, id, prop, node[c.Field], schema); !reflect.DeepEqual(got, c.Expected) {
				t.Errorf("[%s] read: expected %v, got %v", id, c.Expected, got)
			}
			if got, want := hashOf(t, node, schema), hashOf(t, map[string]interface{}{"$type": doc.Class, c.Field: c.Expected}, schema); got != want {
				t.Errorf("[%s] what was read hashes as %s, its lexical forms as %s", id, got, want)
			}

			wire, err := Serialize(node, schema)
			if err != nil {
				t.Errorf("[%s] write what was read: %v", id, err)
				continue
			}
			again, err := Deserialize(decode(mustMarshal(t, wire)), schema)
			if err != nil {
				t.Errorf("[%s] read what was written: %v", id, err)
			} else if got := lexicalOf(t, id, prop, again[c.Field], schema); !reflect.DeepEqual(got, c.Expected) {
				t.Errorf("[%s] round trip: expected %v, got %v", id, c.Expected, got)
			}
		}
	}

	for _, c := range doc.WriteCases {
		total++
		prop, ok := cls.Props[c.Field]
		if !ok {
			t.Fatalf("[%s] field %s is not modeled on %s", c.ID, c.Field, doc.Class)
		}
		node := map[string]interface{}{"$type": doc.Class, c.Field: exactValue(t, prop, c.Value, schema)}
		wire, err := Serialize(node, schema)
		if c.ExpectError {
			if !refused(err) {
				t.Errorf("[%s] write: expected a [json-number-form] refusal, got %v (err %v)", c.ID, wire, err)
			}
			continue
		}
		if err != nil {
			t.Errorf("[%s] write: %v", c.ID, err)
			continue
		}
		var want interface{}
		decodeNumberAware(t, []byte(c.JSON), &want)
		if got := asJSON(t, wire[c.Field]); !reflect.DeepEqual(got, want) {
			t.Errorf("[%s] write: expected %s, got %v", c.ID, c.JSON, got)
		}
	}

	if len(doc.ReadCases) == 0 || len(doc.WriteCases) == 0 {
		t.Fatalf("json-numbers vectors carry no cases")
	}
	t.Logf("codec-vectors-json-numbers.json: %d read cases (each decoded two ways), %d write cases, %d runs",
		len(doc.ReadCases), len(doc.WriteCases), total)
}

// hashOf is the content hash of node, as the one subject of a probe package.
func hashOf(t *testing.T, node map[string]interface{}, schema CodecSchema) string {
	t.Helper()
	subject := maps.Clone(node)
	subject["$id"] = "probe.example.com/data@1.0.0/n"
	hash, err := ContentHash([]map[string]interface{}{subject}, schema,
		PackageContext{Publisher: "probe.example.com", PackageName: "data", Version: "1.0.0"})
	if err != nil {
		t.Fatalf("hash %v: %v", node, err)
	}
	return hash
}

func mustMarshal(t *testing.T, v interface{}) []byte {
	t.Helper()
	data, err := json.Marshal(v)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	return data
}
