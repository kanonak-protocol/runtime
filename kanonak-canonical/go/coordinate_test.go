package canonical

import "testing"

func TestCoordinateParseVectors(t *testing.T) {
	doc := readVectors(t, "coordinate-vectors.json")
	for _, raw := range doc["parseVectors"].([]interface{}) {
		v := raw.(map[string]interface{})
		id := v["id"].(string)
		input := v["input"].(string)
		expectError := false
		if e, ok := v["expectError"].(bool); ok {
			expectError = e
		}
		got, err := ParseCoordinate(input)
		if expectError {
			if err == nil {
				t.Errorf("[%s] expected error, got %+v", id, got)
			}
			continue
		}
		if err != nil {
			t.Errorf("[%s] unexpected error: %v", id, err)
			continue
		}
		e := v["expected"].(map[string]interface{})
		publisher := e["publisher"].(string)
		pkg := e["package"].(string)
		name := e["name"].(string)
		if got.Publisher != publisher || got.Package != pkg || got.Name != name {
			t.Errorf("[%s] expected %s/%s/%s, got %s/%s/%s",
				id, publisher, pkg, name, got.Publisher, got.Package, got.Name)
		}
		if ev, ok := e["version"].(map[string]interface{}); ok {
			if got.Version == nil {
				t.Errorf("[%s] expected version, got nil", id)
			} else if got.Version.Major != int(ev["major"].(float64)) ||
				got.Version.Minor != int(ev["minor"].(float64)) ||
				got.Version.Patch != int(ev["patch"].(float64)) {
				t.Errorf("[%s] version expected %v, got %+v", id, ev, *got.Version)
			}
		} else if got.Version != nil {
			t.Errorf("[%s] expected versionless, got %+v", id, *got.Version)
		}
		expectedKey := publisher + "/" + pkg + "/" + name
		if key, err := VersionlessKey(input); err != nil || key != expectedKey {
			t.Errorf("[%s] versionlessKey expected %q, got %q (err %v)", id, expectedKey, key, err)
		}
		if ln, err := LocalName(input); err != nil || ln != name {
			t.Errorf("[%s] localName expected %q, got %q (err %v)", id, name, ln, err)
		}
	}
}

func TestCoordinateLenientKeyVectors(t *testing.T) {
	doc := readVectors(t, "coordinate-vectors.json")
	for _, raw := range doc["lenientKeyVectors"].([]interface{}) {
		v := raw.(map[string]interface{})
		id := v["id"].(string)
		input := v["input"].(string)
		got, ok := LenientVersionlessKey(input)
		if expected, isString := v["expected"].(string); isString {
			if !ok || got != expected {
				t.Errorf("[%s] expected %q, got %q (ok=%v)", id, expected, got, ok)
			}
		} else if ok {
			t.Errorf("[%s] expected no key, got %q", id, got)
		}
	}
}
