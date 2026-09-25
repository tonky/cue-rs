// Reference adapter for the fixture revision pinned in go.mod.
// It never rewrites fixtures or takes expected values from cue-rs.
package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"math/big"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"cuelang.org/go/cue"
	"cuelang.org/go/cue/ast"
	"cuelang.org/go/cue/cuecontext"
	cueerrors "cuelang.org/go/cue/errors"
	"cuelang.org/go/cue/load"
	"cuelang.org/go/cue/parser"
	"cuelang.org/go/cue/token"
	"cuelang.org/go/internal"
	"cuelang.org/go/internal/core/adt"
	"golang.org/x/tools/txtar"
)

const revision = "635e4bb441b29b0b8a3d754188b8edefe4012d1d"

type Check struct {
	ID        string `json:"id"`
	File      string `json:"file"`
	Path      []any  `json:"path"`
	Operation string `json:"operation"`
	Expected  any    `json:"expected"`
	Status    string `json:"status,omitempty"`
	Reason    string `json:"reason,omitempty"`
}
type Plan struct {
	Revision string  `json:"revision"`
	SHA256   string  `json:"sha256"`
	Checks   []Check `json:"checks"`
}
type planner struct {
	plan    Plan
	root    cue.Value
	ctx     *cue.Context
	serial  int
	blocked string
}

func main() {
	if len(os.Args) == 2 && os.Args[1] == "version" {
		fmt.Println(revision)
		return
	}
	if len(os.Args) != 4 || os.Args[1] != "plan" {
		panic("usage: conformance-oracle plan ARCHIVE ISOLATED_DIRECTORY")
	}
	data, err := os.ReadFile(os.Args[2])
	must(err)
	dir, err := filepath.Abs(os.Args[3])
	must(err)
	must(json.NewEncoder(os.Stdout).Encode(buildPlan(data, dir)))
}

func buildPlan(data []byte, dir string) Plan {
	archive := txtar.Parse(data)
	p := planner{plan: Plan{Revision: revision, SHA256: fmt.Sprintf("%x", sha256.Sum256(data)), Checks: []Check{}}, ctx: cuecontext.New(cuecontext.EvaluatorVersion(cuecontext.EvalV3))}
	overlay := map[string]load.Source{}
	args := []string{}
	for _, f := range archive.Files {
		if strings.HasPrefix(f.Name, "out/") {
			continue
		}
		overlay[filepath.Join(dir, f.Name)] = load.FromBytes(f.Data)
		if !strings.Contains(f.Name, "/") && strings.HasSuffix(f.Name, ".cue") {
			args = append(args, f.Name)
		}
	}
	if len(args) == 0 {
		p.add("", nil, "compile", nil, "unsupported", "archive has no root CUE input")
	} else {
		instances := load.Instances(args, &load.Config{Dir: dir, ModuleRoot: dir, Overlay: overlay, Env: []string{}})
		if len(instances) != 1 || instances[0].Err != nil {
			reason := "expected one package instance"
			if len(instances) > 0 && instances[0].Err != nil {
				reason = instances[0].Err.Error()
			}
			p.blocked = "reference compilation: " + reason
		} else {
			p.root = p.ctx.BuildInstance(instances[0])
		}
		// Configuration directives need explicit adapters. Do not run their checks
		// in a default context and silently count them as verified.
		for _, line := range strings.Split(string(archive.Comment), "\n") {
			if strings.HasPrefix(strings.TrimSpace(line), "#") && !strings.HasPrefix(strings.TrimSpace(line), "# ") {
				p.add("", nil, "configuration", nil, "unsupported", line)
			}
		}
		for _, f := range archive.Files {
			if strings.Contains(f.Name, "/") || !strings.HasSuffix(f.Name, ".cue") {
				continue
			}
			parsed, e := parser.ParseFile(f.Name, f.Data, parser.ParseComments)
			if e != nil {
				p.add(f.Name, nil, "compile", nil, "unsupported", "reference parse error: "+e.Error())
				continue
			}
			p.walk(parsed, f.Name, nil)
		}
		// A differential concrete export is useful even without inline assertions,
		// but it never replaces golden/partial-evaluation coverage.
		if p.blocked == "" {
			if b, e := p.root.MarshalJSON(); e == nil {
				var expected any
				decoder := json.NewDecoder(bytes.NewReader(b))
				decoder.UseNumber()
				must(decoder.Decode(&expected))
				p.add("", nil, "export", expected, "", "")
			}
		}
		if len(p.plan.Checks) == 0 {
			p.add("", nil, "partial", nil, "unsupported", "no supported inline assertions or concrete export")
		}
		for _, f := range archive.Files {
			if !strings.HasPrefix(f.Name, "out/") {
				continue
			}
			if strings.HasSuffix(f.Name, "/stats") {
				p.add(f.Name, nil, "implementation", nil, "not_applicable", "upstream allocation statistics")
				continue
			}
			// Documentary errors in inline fixtures are represented by the inline checks.
			// All other outputs require the operation-specific legacy golden adapter.
			if f.Name == "out/errors.txt" && p.serial > 0 && hasInline(archive) {
				continue
			}
			if f.Name == "out/todo.txt" || f.Name == "out/diff-v2-v3.txt" {
				p.add(f.Name, nil, "implementation", nil, "not_applicable", "upstream migration notes")
				continue
			}
			p.add(f.Name, nil, "golden", nil, "unsupported", "golden adapter required for "+f.Name)
		}
	}
	return p.plan
}
func hasInline(a *txtar.Archive) bool {
	for _, f := range a.Files {
		if !strings.HasPrefix(f.Name, "out/") && strings.Contains(string(f.Data), "@test(") {
			return true
		}
	}
	return false
}
func must(e error) {
	if e != nil {
		fmt.Fprintln(os.Stderr, e)
		os.Exit(2)
	}
}
func (p *planner) add(file string, path []any, op string, want any, status, reason string) {
	p.serial++
	if path == nil {
		path = []any{}
	}
	p.plan.Checks = append(p.plan.Checks, Check{fmt.Sprintf("%s:%04d", file, p.serial), file, path, op, want, status, reason})
}
func appendPath(path []any, s any) []any { r := append([]any{}, path...); return append(r, s) }
func (p *planner) walk(node ast.Node, file string, path []any) {
	switch n := node.(type) {
	case *ast.File:
		for _, d := range n.Decls {
			p.walk(d, file, path)
		}
	case *ast.StructLit:
		for _, d := range n.Elts {
			p.walk(d, file, path)
		}
	case *ast.Field:
		label, _, err := ast.LabelName(n.Label)
		if err != nil {
			ast.Walk(n, func(x ast.Node) bool {
				if a, ok := x.(*ast.Attribute); ok && a.Name() == "test" {
					p.add(file, path, "inline", nil, "unsupported", "dynamic or pattern assertion path: "+a.Text)
				}
				return true
			}, nil)
			return
		}
		next := appendPath(path, label)
		for _, a := range n.Attrs {
			p.attribute(a, file, next)
		}
		p.walk(n.Value, file, next)
	case *ast.ListLit:
		for i, e := range n.Elts {
			p.walk(e, file, appendPath(path, i))
		}
	case *ast.Attribute:
		p.attribute(n, file, path)
	case *ast.EmbedDecl:
		p.walk(n.Expr, file, path)
	default:
		// Expressions can contain annotated structs under conjunctions or
		// comprehensions. Their runtime path requires a separate adapter.
		ast.Walk(node, func(x ast.Node) bool {
			if a, ok := x.(*ast.Attribute); ok && a.Name() == "test" {
				p.add(file, path, "inline", nil, "unsupported", "assertion inside expression: "+a.Text)
			}
			return true
		}, nil)
	}
}
func (p *planner) lookup(path []any) cue.Value {
	sels := []cue.Selector{}
	for _, s := range path {
		switch x := s.(type) {
		case string:
			if strings.HasPrefix(x, "#") {
				sels = append(sels, cue.Def(strings.TrimPrefix(x, "#")))
			} else if strings.HasPrefix(x, "_") {
				return cue.Value{}
			} else {
				sels = append(sels, cue.Str(x))
			}
		case int:
			sels = append(sels, cue.Index(x))
		}
	}
	return p.root.LookupPath(cue.MakePath(sels...))
}
func (p *planner) attribute(a *ast.Attribute, file string, path []any) {
	if a.Name() != "test" {
		return
	}
	attr := internal.ParseAttr(a)
	if attr.Err != nil || len(attr.Fields) == 0 {
		p.add(file, path, "inline", nil, "oracle_mismatch", "invalid/empty attribute: "+a.Text)
		return
	}
	first := attr.Fields[0]
	directive := first.Key()
	if directive == "" {
		directive = first.Value()
	}
	if directive == "shareID" || directive == "debug" || directive == "debugCheck" {
		p.add(file, path, directive, nil, "not_applicable", "upstream internal representation check")
		return
	}
	for _, segment := range path {
		if name, ok := segment.(string); ok && strings.HasPrefix(name, "_") {
			p.add(file, path, directive, nil, "unsupported", "hidden selector needs package identity")
			return
		}
	}
	if p.blocked != "" {
		p.add(file, path, directive, nil, "oracle_mismatch", p.blocked)
		return
	}
	if strings.Contains(directive, ":") {
		p.add(file, path, directive, nil, "unsupported", "version/todo directive: "+a.Text)
		return
	}
	if directive == "err" {
		p.errorAttribute(attr, file, path, a.Text)
		return
	}
	// Only exact supported forms are accepted. In particular pos, at, skip,
	// nested err and todo cannot disappear while an assertion turns green.
	for _, f := range attr.Fields[1:] {
		if f.Key() != "" {
			p.add(file, path, directive, nil, "unsupported", "directive options need an adapter: "+a.Text)
			return
		}
	}
	val := p.lookup(path)
	if !val.Exists() {
		p.add(file, path, directive, nil, "oracle_mismatch", "reference assertion path does not exist")
		return
	}
	switch directive {
	case "eq":
		if len(attr.Fields) != 2 {
			p.add(file, path, directive, nil, "unsupported", "eq expects one literal expression: "+a.Text)
			return
		}
		e, err := parser.ParseExpr("expected", attr.Fields[1].Text())
		if err != nil {
			p.add(file, path, directive, nil, "oracle_mismatch", err.Error())
			return
		}
		want, err := literalValue(e, p.ctx)
		if err != nil {
			p.add(file, path, directive, nil, "unsupported", err.Error())
			return
		}
		actual, err := snapshot(val, 0)
		if err != nil {
			p.add(file, path, directive, nil, "unsupported", "reference value: "+err.Error())
			return
		}
		x, _ := json.Marshal(want)
		y, _ := json.Marshal(actual)
		if string(x) != string(y) {
			p.add(file, path, directive, want, "oracle_mismatch", "literal assertion differs from pinned reference")
			return
		}
		p.add(file, path, "value", want, "", "")
	case "kind":
		if len(attr.Fields) != 1 {
			p.add(file, path, directive, nil, "unsupported", a.Text)
			return
		}
		want := strings.Split(first.Value(), "|")
		sort.Strings(want)
		got := strings.Split(strings.Trim(val.IncompleteKind().String(), "()"), "|")
		sort.Strings(got)
		if strings.Join(want, "|") != strings.Join(got, "|") {
			p.add(file, path, directive, want, "oracle_mismatch", "kind differs from reference")
			return
		}
		p.add(file, path, "kind", got, "", "")
	case "closed":
		if len(attr.Fields) != 1 {
			p.add(file, path, directive, nil, "unsupported", a.Text)
			return
		}
		want := first.Value() != "false"
		got := val.IsClosed()
		if want != got {
			p.add(file, path, directive, want, "oracle_mismatch", "closedness differs from reference")
			return
		}
		p.add(file, path, "closed", want, "", "")
	default:
		p.add(file, path, directive, nil, "unsupported", "directive adapter required: "+a.Text)
	}
}

// Restrict expected expressions to literal trees. Never compile an arbitrary
// computation on both sides of an equality assertion.
func literalValue(e ast.Expr, ctx *cue.Context) (any, error) {
	switch n := e.(type) {
	case *ast.BasicLit:
		return snapshot(ctx.BuildExpr(n), 0)
	case *ast.Ident:
		if n.Name == "true" || n.Name == "false" || n.Name == "null" {
			return snapshot(ctx.BuildExpr(n), 0)
		}
	case *ast.UnaryExpr:
		if n.Op == token.SUB {
			if _, ok := n.X.(*ast.BasicLit); ok {
				return snapshot(ctx.BuildExpr(n), 0)
			}
		}
	case *ast.ParenExpr:
		return literalValue(n.X, ctx)
	case *ast.StructLit:
		fields := map[string]any{}
		for _, d := range n.Elts {
			f, ok := d.(*ast.Field)
			if !ok || len(f.Attrs) > 0 || f.Constraint != token.ILLEGAL {
				return nil, fmt.Errorf("abstract or annotated expected struct")
			}
			name, _, err := ast.LabelName(f.Label)
			if err != nil || strings.HasPrefix(name, "_") || strings.HasPrefix(name, "#") {
				return nil, fmt.Errorf("non-regular expected field")
			}
			if _, ok := fields[name]; ok {
				return nil, fmt.Errorf("repeated expected field")
			}
			v, err := literalValue(f.Value, ctx)
			if err != nil {
				return nil, err
			}
			fields[name] = v
		}
		return map[string]any{"kind": "struct", "fields": fields}, nil
	case *ast.ListLit:
		values := []any{}
		for _, x := range n.Elts {
			e, ok := x.(ast.Expr)
			if !ok {
				return nil, fmt.Errorf("nonliteral list")
			}
			v, err := literalValue(e, ctx)
			if err != nil {
				return nil, err
			}
			values = append(values, v)
		}
		return map[string]any{"kind": "list", "items": values}, nil
	}
	return nil, fmt.Errorf("abstract or computed expected expression requires a structural adapter")
}
func snapshot(v cue.Value, depth int) (any, error) {
	if depth > 128 {
		return nil, fmt.Errorf("observation depth limit")
	}
	// Value.Expr panics for some builtin-produced lists at the pinned revision.
	// Inspect evaluated structure without reconstructing source conjuncts.
	if core := v.Core(); core.V != nil {
		switch core.V.DerefValue().BaseValue.(type) {
		case *adt.Disjunction, *adt.Conjunction:
			return nil, fmt.Errorf("disjunction/conjunction structure")
		}
	}
	switch v.Kind() {
	case cue.StructKind:
		it, err := v.Fields(cue.All(), cue.Patterns(true))
		if err != nil {
			return nil, err
		}
		fields := map[string]any{}
		for it.Next() {
			s := it.Selector()
			if s.IsConstraint() || s.IsDefinition() || (s.LabelType() == cue.HiddenLabel || s.LabelType() == cue.HiddenDefinitionLabel) {
				return nil, fmt.Errorf("optional, hidden or definition fields")
			}
			x, e := snapshot(it.Value(), depth+1)
			if e != nil {
				return nil, e
			}
			fields[s.Unquoted()] = x
		}
		// Pattern/required constraints are not faithfully represented by JSON.

		return map[string]any{"kind": "struct", "fields": fields}, nil
	case cue.ListKind:
		if !v.Len().IsConcrete() {
			return nil, fmt.Errorf("open list")
		}
		it, e := v.List()
		if e != nil {
			return nil, e
		}
		items := []any{}
		for it.Next() {
			x, e := snapshot(it.Value(), depth+1)
			if e != nil {
				return nil, e
			}
			items = append(items, x)
		}
		return map[string]any{"kind": "list", "items": items}, nil
	case cue.IntKind:
		return scalar(v, "int")
	case cue.FloatKind:
		return scalar(v, "float")
	case cue.StringKind:
		x, e := v.String()
		return map[string]any{"kind": "string", "value": x}, e
	case cue.BoolKind:
		x, e := v.Bool()
		return map[string]any{"kind": "bool", "value": x}, e
	case cue.NullKind:
		return map[string]any{"kind": "null"}, nil
	default:
		return nil, fmt.Errorf("non-concrete or unsupported kind %s", v.IncompleteKind())
	}
}
func scalar(v cue.Value, kind string) (any, error) {
	b, e := v.MarshalJSON()
	if e != nil {
		return nil, e
	}
	return map[string]any{"kind": "number", "value": canonicalNumber(string(b))}, nil
}

func (p *planner) errorAttribute(attr *internal.Attr, file string, path []any, raw string) {
	for _, f := range attr.Fields[1:] {
		if f.Key() == "at" || f.Key() == "suberr" || f.Key() == "any" || f.Key() == "" {
			p.add(file, path, "err", nil, "unsupported", "error selector or multi-error adapter: "+raw)
			return
		}
	}
	v := p.lookup(path)
	code := ""
	if core := v.Core(); core.V != nil {
		if b := core.V.Bottom(); b != nil {
			code = b.Code.String()
		}
	}
	if code == "" {
		p.add(file, path, "err", nil, "oracle_mismatch", "reference does not report an error at assertion path")
		return
	}
	specified := false
	for _, f := range attr.Fields[1:] {
		switch f.Key() {
		case "code":
			codes := strings.Split(strings.Trim(f.Value(), "()"), "|")
			matches := false
			for _, c := range codes {
				if strings.TrimSpace(c) == code {
					matches = true
				}
			}
			if !matches {
				p.add(file, path, "error_code", f.Value(), "oracle_mismatch", "reference error code is "+code)
				return
			}
			specified = true
		case "path":
			expected := f.Value()
			paths := []string{}
			for _, e := range cueerrors.Errors(diagnostic(v)) {
				paths = append(paths, strings.Join(e.Path(), "."))
			}
			sort.Strings(paths)
			parts := []string{}
			for _, x := range path {
				parts = append(parts, fmt.Sprint(x))
			}
			prefix := strings.Join(parts, ".") + "."
			matches := len(paths) == 1 && (paths[0] == expected || strings.TrimPrefix(paths[0], prefix) == expected)
			if !matches {
				p.add(file, path, "error_paths", paths, "oracle_mismatch", fmt.Sprintf("reference error paths %v do not match %s", paths, expected))
			} else {
				p.add(file, path, "error_paths", paths, "", "")
			}
		default:
			p.add(file, path, "err/"+f.Key(), nil, "unsupported", "error property adapter required: "+f.Text())
		}
	}
	if specified {
		p.add(file, path, "error_code", code, "", "")
	} else {
		p.add(file, path, "error_present", true, "", "")
	}
}

// Keep exact decimal identity without expanding large exponents.
func canonicalNumber(s string) string {
	parts := strings.FieldsFunc(s, func(r rune) bool { return r == 'e' || r == 'E' })
	exponent := new(big.Int)
	if len(parts) > 1 {
		exponent.SetString(parts[1], 10)
	}
	mantissa := parts[0]
	negative := strings.HasPrefix(mantissa, "-")
	mantissa = strings.TrimPrefix(mantissa, "-")
	pair := strings.SplitN(mantissa, ".", 2)
	fraction := ""
	if len(pair) > 1 {
		fraction = pair[1]
	}
	digits := strings.TrimLeft(pair[0]+fraction, "0")
	if digits == "" {
		return "0e0"
	}
	significant := strings.TrimRight(digits, "0")
	exponent.Add(exponent, big.NewInt(int64(len(digits)-len(significant)-len(fraction))))
	sign := ""
	if negative {
		sign = "-"
	}
	return sign + significant + "e" + exponent.String()
}

// Value.Err omits incomplete errors; the inline runner also observes the
// evaluated bottom when checking error properties in abstract definitions.
func diagnostic(v cue.Value) error {
	if err := v.Err(); err != nil {
		return err
	}
	if core := v.Core(); core.V != nil {
		if b := core.V.Bottom(); b != nil {
			return b.Err
		}
	}
	return nil
}
