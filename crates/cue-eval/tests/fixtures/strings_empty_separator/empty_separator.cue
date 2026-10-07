import "strings"

split: {
	a: strings.Split("abc", "")
	b: strings.Split("", "")
	c: strings.Split("héllo", "")
	d: strings.Split("", ",")
	e: strings.Split("a,b", ",")
}
splitN: {
	a: strings.SplitN("abc", "", 2)
	b: strings.SplitN("abc", "", -1)
	c: strings.SplitN("abc", "", 0)
	d: strings.SplitN("abc", "", 5)
	e: strings.SplitN("", "", 1)
	f: strings.SplitN("a,b,c", ",", 2)
}
count: {
	a: strings.Count("héllo", "")
	b: strings.Count("", "")
}
replace: {
	a: strings.Replace("abc", "", "-", -1)
	b: strings.Replace("abc", "", "-", 2)
	d: strings.Replace("", "", "-", -1)
}
index: {
	a: strings.Index("abc", "")
	b: strings.LastIndex("abc", "")
	c: strings.LastIndex("héllo", "")
	d: strings.Index("héllo", "l")
}
misc: {
	a: strings.Contains("abc", "")
	b: strings.ContainsAny("abc", "")
	c: strings.HasPrefix("abc", "")
	d: strings.HasSuffix("abc", "")
	e: strings.Trim("abc", "")
	f: strings.TrimLeft("abc", "")
	g: strings.TrimRight("abc", "")
	h: strings.TrimPrefix("abc", "")
	i: strings.TrimSuffix("abc", "")
	j: strings.Join([], "")
	k: strings.Join(["a", "b"], "")
	l: strings.Fields("")
	m: strings.Repeat("", 3)
	r: strings.Runes("")
	s: strings.ToUpper("")
	t: strings.Compare("", "")
}
