// Drives the shared wire vectors through the Go kanonak-wire port.
package wire

import (
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"testing"
)

// Go writer numeric params are exact-width types (byte/uint16/uint32) and the
// utf16-strings trap is not how Go strings work: none of the representability
// capabilities apply.
var capabilities = map[string]bool{}

func mustHex(t *testing.T, s string) []byte {
	t.Helper()
	b, err := hex.DecodeString(s)
	if err != nil {
		t.Fatalf("bad hex in vectors: %q: %v", s, err)
	}
	return b
}

// checkError verifies a returned error against an expectError clause.
// Returns a diagnostic string, or "" if it matches.
func checkError(err error, wantKind string, wantOffset *float64) string {
	var we *WireError
	if !errors.As(err, &we) {
		return fmt.Sprintf("expected a *WireError, got %T (%v)", err, err)
	}
	if we.Kind != wantKind {
		return fmt.Sprintf("expected error kind %s, got %s (%v)", wantKind, we.Kind, we)
	}
	if wantOffset != nil {
		if !we.HasOffset {
			return fmt.Sprintf("expected error offset %v, got no offset (%v)", *wantOffset, we)
		}
		if float64(we.Offset) != *wantOffset {
			return fmt.Sprintf("expected error offset %v, got %d (%v)", *wantOffset, we.Offset, we)
		}
	}
	return ""
}

// runReadOp executes one reader op and returns the observed value in the
// vectors' comparison domain: float64 for numeric ops, string for text ops,
// lowercase hex string for byte ops, nil for expectEnd.
func runReadOp(r *WireReader, op map[string]any) (any, error) {
	name, _ := op["op"].(string)
	n := 0
	if f, ok := op["n"].(float64); ok {
		n = int(f)
	}
	switch name {
	case "u8":
		v, err := r.U8()
		return float64(v), err
	case "u16be":
		v, err := r.U16BE()
		return float64(v), err
	case "u32be":
		v, err := r.U32BE()
		return float64(v), err
	case "bytes":
		v, err := r.Bytes(n)
		if err != nil {
			return nil, err
		}
		return hex.EncodeToString(v), nil
	case "uuid":
		return r.UUID()
	case "utf8":
		return r.UTF8(n)
	case "lenPrefixedBytes16":
		v, err := r.LenPrefixedBytes16()
		if err != nil {
			return nil, err
		}
		return hex.EncodeToString(v), nil
	case "rest":
		return hex.EncodeToString(r.Rest()), nil
	case "remaining":
		return float64(r.Remaining()), nil
	case "expectEnd":
		return nil, r.ExpectEnd()
	default:
		panic(fmt.Sprintf("conformance: unknown read op %q", name))
	}
}

func runWriteOp(w *WireWriter, op map[string]any) error {
	name, _ := op["op"].(string)
	switch name {
	case "u8":
		w.U8(byte(op["value"].(float64)))
		return nil
	case "u16be":
		w.U16BE(uint16(op["value"].(float64)))
		return nil
	case "u32be":
		w.U32BE(uint32(op["value"].(float64)))
		return nil
	case "bytes":
		b, err := hex.DecodeString(op["hex"].(string))
		if err != nil {
			panic(fmt.Sprintf("conformance: bad hex %q", op["hex"]))
		}
		w.Bytes(b)
		return nil
	case "uuid":
		return w.UUID(op["value"].(string))
	case "utf8":
		return w.UTF8(op["value"].(string))
	case "lenPrefixedBytes16":
		b, err := hex.DecodeString(op["hex"].(string))
		if err != nil {
			panic(fmt.Sprintf("conformance: bad hex %q", op["hex"]))
		}
		return w.LenPrefixedBytes16(b)
	default:
		panic(fmt.Sprintf("conformance: unknown write op %q", name))
	}
}

// expectErrorClause extracts an op's expectError as (kind, offset, present).
func expectErrorClause(op map[string]any) (string, *float64, bool) {
	raw, ok := op["expectError"].(map[string]any)
	if !ok {
		return "", nil, false
	}
	kind, _ := raw["kind"].(string)
	var offset *float64
	if f, ok := raw["offset"].(float64); ok {
		offset = &f
	}
	return kind, offset, true
}

func TestWireVectors(t *testing.T) {
	data, err := os.ReadFile("../vectors/wire-vectors.json")
	if err != nil {
		t.Fatalf("read vectors: %v", err)
	}
	var doc struct {
		ReadVectors []struct {
			ID       string           `json:"id"`
			Bytes    string           `json:"bytes"`
			Ops      []map[string]any `json:"ops"`
			Requires string           `json:"requires"`
		} `json:"readVectors"`
		WriteVectors []struct {
			ID            string           `json:"id"`
			Ops           []map[string]any `json:"ops"`
			ExpectedBytes *string          `json:"expectedBytes"`
			Requires      string           `json:"requires"`
		} `json:"writeVectors"`
	}
	if err := json.Unmarshal(data, &doc); err != nil {
		t.Fatalf("parse vectors: %v", err)
	}

	pass, fails, skipped := 0, 0, 0
	fail := func(format string, args ...any) {
		t.Errorf(format, args...)
		fails++
	}

	for _, v := range doc.ReadVectors {
		if v.Requires != "" && !capabilities[v.Requires] {
			skipped++
			continue
		}
		r := NewWireReader(mustHex(t, v.Bytes))
		before := fails
		for _, op := range v.Ops {
			opname, _ := op["op"].(string)
			if wantKind, wantOffset, hasErr := expectErrorClause(op); hasErr {
				got, err := runReadOp(r, op)
				if err == nil {
					fail("%s: %s: expected %s, got a value (%v)", v.ID, opname, wantKind, got)
				} else if diag := checkError(err, wantKind, wantOffset); diag != "" {
					fail("%s: %s: %s", v.ID, opname, diag)
				}
				break
			}
			got, err := runReadOp(r, op)
			if err != nil {
				fail("%s: %s: threw %v", v.ID, opname, err)
				break
			}
			if expected, ok := op["expected"]; ok && got != expected {
				fail("%s: %s: expected %v, got %v", v.ID, opname, expected, got)
				break
			}
		}
		if fails == before {
			pass++
		}
	}

	for _, v := range doc.WriteVectors {
		if v.Requires != "" && !capabilities[v.Requires] {
			skipped++
			continue
		}
		w := NewWireWriter()
		before := fails
		for _, op := range v.Ops {
			opname, _ := op["op"].(string)
			if wantKind, wantOffset, hasErr := expectErrorClause(op); hasErr {
				err := runWriteOp(w, op)
				if err == nil {
					fail("%s: %s: expected %s, got success", v.ID, opname, wantKind)
				} else if diag := checkError(err, wantKind, wantOffset); diag != "" {
					fail("%s: %s: %s", v.ID, opname, diag)
				}
				break
			}
			if err := runWriteOp(w, op); err != nil {
				fail("%s: %s: threw %v", v.ID, opname, err)
				break
			}
		}
		if fails == before && v.ExpectedBytes != nil {
			got := hex.EncodeToString(w.ToBytes())
			if got != *v.ExpectedBytes {
				fail("%s: expected bytes %s, got %s", v.ID, *v.ExpectedBytes, got)
			}
		}
		if fails == before {
			pass++
		}
	}

	total := len(doc.ReadVectors) + len(doc.WriteVectors)
	t.Logf("wire-vectors: %d/%d pass (%d skipped)", pass, total, skipped)
	if fails > 0 {
		t.Fatalf("%d VECTOR(S) FAILED", fails)
	}
}
