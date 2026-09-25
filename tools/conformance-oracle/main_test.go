package main

import (
	"encoding/json"
	"os"
	"testing"
)

func TestOperationSelection(t *testing.T) {
	tests := []struct{ name, source, op, status string }{
		{"concrete", "a: 42 @test(eq, 42)", "value", ""},
		{"abstract", "a: int @test(eq, int)", "eq", "unsupported"},
		{"wrongExpected", "a: 42 @test(eq, 43)", "eq", "oracle_mismatch"},
		{"wrongErrorPath", "bad: 1 & 2\ngood: 1 @test(err, code=eval)", "err", "oracle_mismatch"},
		{"wrongErrorCategory", "bad: 1 & 2 @test(err, code=cycle)", "error_code", "oracle_mismatch"},
		{"error", "bad: 1 & 2 @test(err, code=eval)", "error_code", ""},
		{"partialErrorCheck", "bad: 1 & 2 @test(err, code=eval, pos=[])", "err/pos", "unsupported"},
		{"default", "a: *1 | 2 @test(eq, 1)", "eq", "unsupported"},
		{"hidden", "_a: 1 @test(eq, 1)", "eq", "unsupported"},
		{"listBuiltin", "import \"list\"\na: list.Repeat([\"b\"], 3) @test(eq, [\"b\",\"b\",\"b\"])", "value", ""},
		{"numericSpelling", "a: 4/2 @test(eq, 2.0)", "value", ""},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			plan := buildPlan([]byte("-- in.cue --\n"+tt.source+"\n"), t.TempDir())
			found := false
			for _, c := range plan.Checks {
				if c.Operation == tt.op && c.Status == tt.status {
					found = true
				}
			}
			if !found {
				b, _ := json.MarshalIndent(plan, "", "  ")
				t.Fatalf("missing %s/%s: %s", tt.op, tt.status, b)
			}
		})
	}
}

func TestPackageAndExactNumber(t *testing.T) {
	p := buildPlan([]byte("-- a.cue --\npackage test\na: b+1 @test(eq, 3)\n-- b.cue --\npackage test\nb: 2\nlarge: 18446744073709551616\n"), t.TempDir())
	for _, c := range p.Checks {
		if c.Status != "" {
			t.Fatalf("unexpected check: %+v", c)
		}
	}
	b, _ := json.Marshal(p)
	var decoded struct {
		Checks []struct {
			Operation string
			Expected  map[string]json.RawMessage
		}
	}
	// The literal check has a structural object, and export retains the integer.
	if err := json.Unmarshal(b, &decoded); err != nil {
		t.Fatal(err)
	}
	found := false
	for _, c := range decoded.Checks {
		if c.Operation == "export" {
			found = true
			if string(c.Expected["large"]) != "18446744073709551616" {
				t.Fatalf("rounded number: %s", b)
			}
		}
	}
	if !found {
		t.Fatal("no export")
	}
}

func TestCanonicalNumbers(t *testing.T) {
	for _, s := range []string{"2", "2.0", "20e-1", "0.02e2"} {
		if canonicalNumber(s) != "2e0" {
			t.Fatal(s)
		}
	}
	if canonicalNumber("-0.0") != "0e0" {
		t.Fatal("negative zero")
	}
	if canonicalNumber("123e999999999999999999999") != "123e999999999999999999999" {
		t.Fatal("large exponent")
	}
}

func TestIncompleteErrorPaths(t *testing.T) {
	for _, name := range []string{"upstream_cue_testdata_eval_required.txtar", "upstream_cue_testdata_export_issue854.txtar"} {
		t.Run(name, func(t *testing.T) {
			data, err := os.ReadFile("../../tests/testdata/" + name)
			if err != nil {
				t.Fatal(err)
			}
			plan := buildPlan(data, t.TempDir())
			found := false
			for _, c := range plan.Checks {
				if c.Operation == "error_paths" {
					found = true
					wantStatus := ""
					// Upstream's inline runner skips its path check when Err()
					// omits an incomplete bottom. Keep the stale annotation visible.
					if len(c.Path) == 3 && c.Path[0] == "issue3918" && c.Path[1] == "noFunction" && c.Path[2] == "x" {
						wantStatus = "oracle_mismatch"
					}
					if c.Status != wantStatus {
						t.Fatalf("%+v", c)
					}
				}
			}
			if !found {
				t.Fatal("missing path observations")
			}
		})
	}
}
