-- Cross-references for the quick preview, as pandoc-crossref and texmark's PDF give them (pandoc's own Lua; no
-- other dependency). Run before citeproc.
--   * display equations: `$$..$$ {#eq:x}`, and the environments equation, align, gather (starred or not), whose
--     `\label{x}` is read and `\nonumber` dropped, become display math pandoc can convert; numbered ones get a
--     `<span class="eqno">(n)</span>` after the math, inside `<div class="eqn" id="x">`
--   * figures `{#fig:x}`, tables `{#tbl:x}`, listings `{#lst:x}`: numbered in order, the caption prefixed
--     "Figure n: ", "Table n: ", "Listing n: "; sections `{#sec:x}`: numbered (1, 1.1, ...) in the heading
--   * `@fig:x`, `@eq:x`, `@tbl:x`, `@sec:x`, `@lst:x` and `\ref{x}`, `\eqref{x}`: the number (an equation's in
--     parentheses), linked to its target; several in one citation: "1 and 2", "1, 2 and 3"; unknown labels give a bold
--     "??"; `\label{x}` elsewhere is dropped
--   * words before the numbers only when the front matter says so, as texmark does (empty: bare numbers):
--     `crossref: {fig: Fig., figs: Figs., eq: Eq., eqs: Eqs., tbl: Table, tbls: Tables, sec: Sect., secs: Sects.,
--     lst: Listing, lsts: Listings}`: "Fig. 1", "Figs. 1 and 2" (a plural left out is the singular)
-- A failure inside leaves the document as it was.

-- the words before a reference, singular and plural, by kind: none until the front matter gives them
local KINDS = {
  fig = { "", "" }, eq = { "", "" }, tbl = { "", "" }, sec = { "", "" }, lst = { "", "" },
}
local NAMES = { fig = "Figure", tbl = "Table", lst = "Listing" }
local WRAP = { equation = false, align = "aligned", gather = "gathered" }
local NBSP = "\xC2\xA0"

local labels, count, hdr = {}, { fig = 0, eq = 0, tbl = 0, sec = 0, lst = 0 }, { 0, 0, 0, 0, 0, 0 }
local number_sections = false

local function kind_of(id)
  local k = id and id:match("^(%a+):")
  return k and KINDS[k] and k or nil
end

local function register(id, kind, n, text, anchor)
  labels[id] = { kind = kind, n = n, text = text, id = anchor or id }
end

local function is_tex(f) return f == "tex" or f == "latex" end

-- remove a trailing `{#id}` and any `\label{id}` from inlines (in place); the label, if any
local function take_label(inlines)
  local found
  local last = inlines[#inlines]
  if last and last.t == "Str" then
    local id = last.text:match("^{#([^%s}]+)}$")
    if id then
      found = id
      inlines:remove(#inlines)
      while #inlines > 0 and (inlines[#inlines].t == "Space" or inlines[#inlines].t == "SoftBreak") do
        inlines:remove(#inlines)
      end
    end
  end
  for j = #inlines, 1, -1 do
    local el = inlines[j]
    if el.t == "RawInline" and is_tex(el.format) then
      local l = el.text:match("^\\label{([^}]*)}$")
      if l then found = found or l; inlines:remove(j) end
    end
  end
  return found
end

-- tex without labels and numbering switches; the labels
local function clean(tex)
  local ls = {}
  tex = tex:gsub("\\label{([^}]*)}", function(l) ls[#ls + 1] = l; return "" end)
  tex = tex:gsub("\\nonumber", ""):gsub("\\notag", "")
  return tex, ls
end

-- `\begin{env}..\end{env}` as (env, starred, body)
local function parse_env(tex)
  local name, body = tex:match("^%s*\\begin{(%a+%*?)}(.*)\\end{%1}%s*$")
  if not name then return nil end
  local base = name:gsub("%*$", "")
  if WRAP[base] == nil then return nil end
  return base, name ~= base, body
end

-- display math of `tex` (a `$$` body or an environment), with its labels: tex, labels, environment, starred
local function prepare(tex)
  local env, starred, body = parse_env(tex)
  local src = env and body or tex
  local out, ls = clean(src)
  if env and WRAP[env] then out = "\\begin{" .. WRAP[env] .. "}" .. out .. "\\end{" .. WRAP[env] .. "}" end
  return out, ls, env, starred
end

local function numbered_equation(math, label, aliases)
  count.eq = count.eq + 1
  local n = count.eq
  for _, l in ipairs(aliases) do register(l, "eq", n, tostring(n), label) end
  if label then register(label, "eq", n, tostring(n), label) end
  local no = pandoc.Span({ pandoc.Str("(" .. n .. ")") }, pandoc.Attr("", { "eqno" }))
  return pandoc.Div({ pandoc.Para({ math, no }) }, pandoc.Attr(label or "", { "eqn" }))
end

-- a paragraph that is one display equation (and its `{#eq:x}`): a Div or Para, else nil
local function equation_para(inlines)
  inlines = pandoc.Inlines(inlines)
  local attr_label = take_label(inlines)
  if #inlines ~= 1 then return nil end
  local el, tex = inlines[1], nil
  if el.t == "Math" and el.mathtype == "DisplayMath" then
    tex = el.text
  elseif el.t == "RawInline" and is_tex(el.format) and parse_env(el.text) then
    tex = el.text
  else
    return nil
  end
  local out, ls, env, starred = prepare(tex)
  local label = attr_label or ls[1]
  local numbered = attr_label ~= nil or (env ~= nil and not starred) or (env == nil and #ls > 0)
  local math = pandoc.Math("DisplayMath", out)
  if numbered then return numbered_equation(math, label, ls) end
  if env or out ~= tex then return pandoc.Para({ math }) end
  return nil
end

local function caption_prefix(el, kind)
  count[kind] = count[kind] + 1
  local n = count[kind]
  local prefix = { pandoc.Str(NAMES[kind]), pandoc.Space(), pandoc.Str(n .. ":"), pandoc.Space() }
  local long = el.caption.long
  local first = long[1]
  if first and (first.t == "Plain" or first.t == "Para") then
    local c = pandoc.List(prefix)
    c:extend(first.content)
    long[1] = pandoc.Plain(c)
  else
    prefix[#prefix] = nil
    prefix[#prefix] = pandoc.Str(n .. "")
    long = pandoc.List({ pandoc.Plain(prefix) })
    long:extend(el.caption.long)
  end
  el.caption.long = long
  return n
end

local function captioned(kind)
  return function(el)
    local label = el.identifier ~= "" and el.identifier or nil
    local first = el.caption.long[1]
    if first and (first.t == "Plain" or first.t == "Para") then
      local content = pandoc.Inlines(first.content)
      local l = take_label(content)
      if l then
        label = label or l
        local long = el.caption.long
        long[1] = pandoc.Plain(content)
        el.caption.long = long
      end
    end
    if kind_of(label) ~= kind then return nil end
    el.identifier = label
    local n = caption_prefix(el, kind)
    register(label, kind, n, tostring(n))
    return el
  end
end

local pass1 = { traverse = "topdown" }

pass1.Figure = captioned("fig")
pass1.Table = captioned("tbl")

pass1.Header = function(h)
  local unnumbered = h.classes:includes("unnumbered")
  local content = pandoc.Inlines(h.content)
  local l = take_label(content)
  if l and kind_of(l) == "sec" then
    h.identifier = l
    h.content = content
  end
  if unnumbered then return h end
  local lv = h.level
  hdr[lv] = hdr[lv] + 1
  for i = lv + 1, 6 do hdr[i] = 0 end
  local first = 1
  while first < lv and hdr[first] == 0 do first = first + 1 end
  local parts = {}
  for i = first, lv do parts[#parts + 1] = tostring(hdr[i]) end
  local text = table.concat(parts, ".")
  if kind_of(h.identifier) == "sec" then register(h.identifier, "sec", hdr[lv], text) end
  if number_sections then
    local c = pandoc.List({ pandoc.Str(text), pandoc.Space() })
    c:extend(h.content)
    h.content = c
  end
  return h
end

pass1.CodeBlock = function(cb)
  if kind_of(cb.identifier) ~= "lst" then return nil end
  count.lst = count.lst + 1
  local n, id = count.lst, cb.identifier
  register(id, "lst", n, tostring(n))
  local title = { pandoc.Str("Listing"), pandoc.Space(), pandoc.Str(n .. "") }
  local cap = cb.attributes["caption"]
  if cap then
    title[#title].text = n .. ":"
    title[#title + 1] = pandoc.Space()
    for _, i in ipairs(pandoc.read(cap, "markdown").blocks[1].content) do title[#title + 1] = i end
  end
  cb.identifier = ""
  cb.attributes["caption"] = nil
  return pandoc.Div({ pandoc.Para({ pandoc.Strong(title) }), cb }, pandoc.Attr(id, { "listing" })), false
end

pass1.Para = function(p)
  local out = equation_para(p.content)
  if out then return out, false end
  return nil
end

pass1.RawBlock = function(b)
  if not is_tex(b.format) then return nil end
  if b.text:match("^%s*\\label{[^}]*}%s*$") then return {} end
  if parse_env(b.text) then
    local out = equation_para({ pandoc.RawInline("tex", b.text) })
    if out then return out, false end
  end
  return nil
end

-- the text of a reference to `e`
local function number_text(e) return e.kind == "eq" and ("(" .. e.text .. ")") or e.text end

local function link(content, e) return pandoc.Link(content, "#" .. e.id) end

local function unknown() return pandoc.Strong({ pandoc.Str("??") }) end

-- parts (lists of inlines) joined as "a", "a and b", "a, b and c"
local function join(parts)
  local out = pandoc.Inlines({})
  for i, p in ipairs(parts) do
    if i > 1 then
      if i == #parts then
        out:insert(pandoc.Space()); out:insert(pandoc.Str("and")); out:insert(pandoc.Space())
      else
        out:insert(pandoc.Str(",")); out:insert(pandoc.Space())
      end
    end
    out:extend(p)
  end
  return out
end

local function resolve_cite(c)
  local items = {}
  for _, ct in ipairs(c.citations) do
    local k = kind_of(ct.id)
    if not k then return nil end
    items[#items + 1] = { kind = k, e = labels[ct.id] }
  end
  if #items == 0 then return nil end
  -- runs of one kind, each with its word (plural for several), the numbers joined
  local groups = {}
  local i = 1
  while i <= #items do
    local k, j = items[i].kind, i
    while j < #items and items[j + 1].kind == k do j = j + 1 end
    local nums = {}
    for g = i, j do
      local e = items[g].e
      nums[#nums + 1] = e and pandoc.Inlines({ link({ pandoc.Str(number_text(e)) }, e) }) or pandoc.Inlines({ unknown() })
    end
    local part = pandoc.Inlines({})
    local word = KINDS[k][j > i and 2 or 1]
    if word ~= "" then part:insert(pandoc.Str(word .. NBSP)) end
    part:extend(join(nums))
    groups[#groups + 1] = part
    i = j + 1
  end
  local out = pandoc.Inlines({})
  local first = c.citations[1].prefix
  if #first > 0 then out:extend(first); out:insert(pandoc.Space()) end
  out:extend(join(groups))
  local suffix = c.citations[#c.citations].suffix
  if #suffix > 0 then out:extend(suffix) end
  return out
end

local pass2 = {}

pass2.Cite = resolve_cite

pass2.RawInline = function(r)
  if not is_tex(r.format) then return nil end
  local cmd, arg = r.text:match("^\\(%a+){([^}]*)}$")
  if cmd == "label" then return {} end
  if cmd == "ref" or cmd == "eqref" then
    local e = labels[arg]
    if not e then return unknown() end
    local t = cmd == "eqref" and ("(" .. e.text .. ")") or e.text
    return link({ pandoc.Str(t) }, e)
  end
  if parse_env(r.text) then
    local out = prepare(r.text)
    return pandoc.Math("DisplayMath", out)
  end
  return nil
end

pass2.Math = function(m)
  if not (m.text:find("\\label", 1, true) or m.text:find("\\nonumber", 1, true) or m.text:find("\\notag", 1, true)
      or parse_env(m.text)) then
    return nil
  end
  local out = prepare(m.text)
  return pandoc.Math(parse_env(m.text) and "DisplayMath" or m.mathtype, out)
end

-- the words of the front matter's `crossref:`
local function read_words(meta)
  local m = meta.crossref
  if type(m) ~= "table" or m.t == "MetaInlines" or m.t == "MetaString" then return end
  for k, words in pairs(KINDS) do
    local one, many = m[k], m[k .. "s"]
    if one ~= nil then words[1] = pandoc.utils.stringify(one); words[2] = words[1] end
    if many ~= nil then words[2] = pandoc.utils.stringify(many) end
  end
end

function Pandoc(doc)
  local ok, res = pcall(function()
    read_words(doc.meta)
    doc:walk({ Header = function(h) if kind_of(h.identifier) == "sec" then number_sections = true end end })
    local d = doc:walk(pass1)
    return d:walk(pass2)
  end)
  if ok and res then return res end
  io.stderr:write("[WARNING] crossref filter: " .. tostring(res) .. "\n")
  return doc
end
