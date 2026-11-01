// The encoding/json binding of the JSON forms of integer and decimal values
// (kanonak-protocol 4.11.0's numbers-in-json, runtime#54): the types a
// generated struct's fields take, so encoding/json reads and writes them as
// Serialize and Deserialize do. int and narrower take int32 and the rest, since
// encoding/json already refuses a string, a fraction, or a value past their
// width for those.

package codec

import (
	"bytes"
	"encoding/json"
	"fmt"
	"math/big"
	"strconv"

	canonical "github.com/kanonak-protocol/runtime/kanonak-canonical/go"
)

// IntegerDatatype is integer or one of its four sign-restricted forms, as the
// type parameter of an Integer, so a field states which datatype it holds and
// its range is checked.
type IntegerDatatype interface {
	// LocalName is the datatype's local name in core-xsd, by which the codec's
	// one table gives its bounds.
	LocalName() string
}

// XsdInteger is xsd:integer, for Integer.
type XsdInteger struct{}

// NonNegativeInteger is xsd:nonNegativeInteger, for Integer.
type NonNegativeInteger struct{}

// PositiveInteger is xsd:positiveInteger, for Integer.
type PositiveInteger struct{}

// NonPositiveInteger is xsd:nonPositiveInteger, for Integer.
type NonPositiveInteger struct{}

// NegativeInteger is xsd:negativeInteger, for Integer.
type NegativeInteger struct{}

func (XsdInteger) LocalName() string         { return "integer" }
func (NonNegativeInteger) LocalName() string { return "nonNegativeInteger" }
func (PositiveInteger) LocalName() string    { return "positiveInteger" }
func (NonPositiveInteger) LocalName() string { return "nonPositiveInteger" }
func (NegativeInteger) LocalName() string    { return "negativeInteger" }

func bindingError(why string) error {
	return fmt.Errorf("codec: %s [json-number-form]", why)
}

func inDatatypeRange(local string, n *big.Int) error {
	form := jsonNumberForms[xsd+local]
	if (form.min != nil && n.Cmp(form.min) < 0) || (form.max != nil && n.Cmp(form.max) > 0) {
		return bindingError(fmt.Sprintf("%s is outside xsd:%s's range", n, local))
	}
	return nil
}

func integerOfDatatypeLexical(local, lexical string) (*big.Int, error) {
	if canon, err := canonical.CanonicalInteger(lexical); err == nil {
		if n, ok := new(big.Int).SetString(canon, 10); ok {
			return n, nil
		}
	}
	return nil, bindingError(fmt.Sprintf("%q is not an xsd:%s lexical form", lexical, local))
}

// readWideInteger reads a wide integer as JSON carries it: from a string
// holding a lexical form of the datatype, or from a whole JSON number within
// ±(2^53 − 1), then checks the datatype's range.
func readWideInteger(local string, data []byte) (*big.Int, error) {
	var n *big.Int
	if len(data) > 0 && data[0] == '"' {
		var s string
		if err := json.Unmarshal(data, &s); err != nil {
			return nil, err
		}
		var err error
		if n, err = integerOfDatatypeLexical(local, s); err != nil {
			return nil, err
		}
	} else if whole, ok := wholeNumber(json.Number(data)); ok && whole.CmpAbs(maxSafeInteger) <= 0 {
		n = whole
	} else {
		return nil, bindingError(fmt.Sprintf(
			"an xsd:%s is read from a string, or from a whole JSON number within ±(2^53 − 1); got %s", local, data))
	}
	if err := inDatatypeRange(local, n); err != nil {
		return nil, err
	}
	return n, nil
}

// Integer is an integer of datatype D, held exactly. JSON carries it as a
// string of its digits, and it reads from a string or a whole number within
// ±(2^53 − 1), inside D's range. The zero value is 0.
type Integer[D IntegerDatatype] struct {
	n *big.Int
}

// ParseInteger is an Integer from any lexical form of D, refused outside D's
// range.
func ParseInteger[D IntegerDatatype](lexical string) (Integer[D], error) {
	var d D
	n, err := integerOfDatatypeLexical(d.LocalName(), lexical)
	if err != nil {
		return Integer[D]{}, err
	}
	return NewInteger[D](n)
}

// NewInteger is an Integer holding n, refused outside D's range.
func NewInteger[D IntegerDatatype](n *big.Int) (Integer[D], error) {
	var d D
	if err := inDatatypeRange(d.LocalName(), n); err != nil {
		return Integer[D]{}, err
	}
	return Integer[D]{n: new(big.Int).Set(n)}, nil
}

// Int is its value, a copy.
func (i Integer[D]) Int() *big.Int {
	if i.n == nil {
		return new(big.Int)
	}
	return new(big.Int).Set(i.n)
}

// String is its canonical digits.
func (i Integer[D]) String() string { return i.Int().String() }

// MarshalJSON writes it as a string of its digits. An Integer is in its
// range once made, but the zero value isn't for every D, so it's checked.
func (i Integer[D]) MarshalJSON() ([]byte, error) {
	var d D
	n := i.Int()
	if err := inDatatypeRange(d.LocalName(), n); err != nil {
		return nil, err
	}
	return json.Marshal(n.String())
}

// UnmarshalJSON reads it from a string or a whole number within ±(2^53 − 1),
// refused outside D's range. A JSON null leaves it as it is, as encoding/json's
// own types do.
func (i *Integer[D]) UnmarshalJSON(data []byte) error {
	if bytes.Equal(data, []byte("null")) {
		return nil
	}
	var d D
	n, err := readWideInteger(d.LocalName(), data)
	if err != nil {
		return err
	}
	i.n = n
	return nil
}

// Long is an xsd:long, which JSON carries as a string of its digits. It reads
// from a string or a whole number within ±(2^53 − 1).
type Long int64

// ParseLong is a Long from any lexical form of xsd:long, refused outside its
// width.
func ParseLong(lexical string) (Long, error) {
	n, err := integerOfDatatypeLexical("long", lexical)
	if err == nil {
		err = inDatatypeRange("long", n)
	}
	if err != nil {
		return 0, err
	}
	return Long(n.Int64()), nil
}

// String is its canonical digits.
func (l Long) String() string { return strconv.FormatInt(int64(l), 10) }

// MarshalJSON writes it as a string of its digits.
func (l Long) MarshalJSON() ([]byte, error) { return json.Marshal(l.String()) }

// UnmarshalJSON reads it from a string or a whole number within ±(2^53 − 1).
// A JSON null leaves it as it is.
func (l *Long) UnmarshalJSON(data []byte) error {
	if bytes.Equal(data, []byte("null")) {
		return nil
	}
	n, err := readWideInteger("long", data)
	if err != nil {
		return err
	}
	*l = Long(n.Int64())
	return nil
}

// UnsignedLong is an xsd:unsignedLong, which JSON carries as a string of its
// digits. It reads from a string or a whole number within ±(2^53 − 1).
type UnsignedLong uint64

// ParseUnsignedLong is an UnsignedLong from any lexical form of
// xsd:unsignedLong, refused outside its width.
func ParseUnsignedLong(lexical string) (UnsignedLong, error) {
	n, err := integerOfDatatypeLexical("unsignedLong", lexical)
	if err == nil {
		err = inDatatypeRange("unsignedLong", n)
	}
	if err != nil {
		return 0, err
	}
	return UnsignedLong(n.Uint64()), nil
}

// String is its canonical digits.
func (u UnsignedLong) String() string { return strconv.FormatUint(uint64(u), 10) }

// MarshalJSON writes it as a string of its digits.
func (u UnsignedLong) MarshalJSON() ([]byte, error) { return json.Marshal(u.String()) }

// UnmarshalJSON reads it from a string or a whole number within ±(2^53 − 1).
// A JSON null leaves it as it is.
func (u *UnsignedLong) UnmarshalJSON(data []byte) error {
	if bytes.Equal(data, []byte("null")) {
		return nil
	}
	n, err := readWideInteger("unsignedLong", data)
	if err != nil {
		return err
	}
	*u = UnsignedLong(n.Uint64())
	return nil
}

// Decimal is an xsd:decimal, held as its canonical digits, since Go's standard
// library has no type holding every value exactly. JSON carries it as a
// string of those digits, and it reads from a string only, so it can't have
// been rounded. The zero value is 0.
type Decimal struct {
	digits string
}

// ParseDecimal is a Decimal from any lexical form of xsd:decimal.
func ParseDecimal(lexical string) (Decimal, error) {
	canon, err := canonical.CanonicalDecimal(lexical)
	if err != nil {
		return Decimal{}, bindingError(fmt.Sprintf("%q is not an xsd:decimal lexical form", lexical))
	}
	return Decimal{digits: canon}, nil
}

// String is its canonical digits.
func (d Decimal) String() string {
	if d.digits == "" {
		zero, _ := canonical.CanonicalDecimal("0")
		return zero
	}
	return d.digits
}

// MarshalJSON writes it as a string of its digits.
func (d Decimal) MarshalJSON() ([]byte, error) { return json.Marshal(d.String()) }

// UnmarshalJSON reads it from a string only, since a number may already have
// been rounded. A JSON null leaves it as it is.
func (d *Decimal) UnmarshalJSON(data []byte) error {
	if bytes.Equal(data, []byte("null")) {
		return nil
	}
	if len(data) == 0 || data[0] != '"' {
		return bindingError(fmt.Sprintf(
			"an xsd:decimal is read from a JSON string only, never a number, which may have been rounded; got %s", data))
	}
	var s string
	if err := json.Unmarshal(data, &s); err != nil {
		return err
	}
	parsed, err := ParseDecimal(s)
	if err != nil {
		return err
	}
	*d = parsed
	return nil
}
