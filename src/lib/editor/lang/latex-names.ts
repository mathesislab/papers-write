// The LaTeX names offered after a backslash: in math and LaTeX environments the usual ones, in the text only the
// structural ones. An entry is a name with its backslash, and the text it stands for when it takes arguments, in the
// syntax of CodeMirror snippets (`${}` is where the cursor goes, `${1}`, `${2}` the fields Tab goes through).

export type LatexName = { name: string; snippet?: string; detail?: string };

const plain = (...names: string[]): LatexName[] => names.map((name) => ({ name: `\\${name}` }));
const args = (name: string, ...fields: string[]): LatexName => ({
  name: `\\${name}`,
  snippet: `\\${name}${fields.map((f) => `{${f}}`).join("")}`,
});

/** The Greek letters and the symbols of math. */
const SYMBOLS = plain(
  ..."alpha beta gamma delta epsilon varepsilon zeta eta theta vartheta iota kappa lambda mu nu xi pi rho varrho sigma tau upsilon phi varphi chi psi omega".split(" "),
  ..."Gamma Delta Theta Lambda Xi Pi Sigma Upsilon Phi Psi Omega".split(" "),
  ..."pm mp times cdot div cdots ldots approx sim simeq propto equiv neq leq geq ll gg infty partial nabla sum prod int oint".split(" "),
  ..."in notin subset subseteq supset cup cap emptyset forall exists to rightarrow leftarrow leftrightarrow Rightarrow Leftrightarrow mapsto".split(" "),
  ..."sin cos tan exp log ln min max lim sup inf circ prime ell hbar quad qquad".split(" "),
);

/** The commands with arguments in math, and the delimiters. */
const MATH: LatexName[] = [
  args("frac", "${1}", "${2}"),
  args("sqrt", "${}"),
  args("mathrm", "${}"),
  args("mathbf", "${}"),
  args("mathcal", "${}"),
  args("mathbb", "${}"),
  args("text", "${}"),
  args("hat", "${}"),
  args("bar", "${}"),
  args("dot", "${}"),
  args("ddot", "${}"),
  args("tilde", "${}"),
  args("vec", "${}"),
  args("overline", "${}"),
  args("underline", "${}"),
  args("operatorname", "${}"),
  args("boldsymbol", "${}"),
  { name: "\\left(", snippet: "\\left( ${} \\right)" },
  { name: "\\left[", snippet: "\\left[ ${} \\right]" },
  { name: "\\left\\{", snippet: "\\left\\{ ${} \\right\\}" },
  { name: "\\right" },
  { name: "\\left" },
  args("label", "${}"),
  { name: "\\begin{aligned}", snippet: "\\begin{aligned}\n  ${}\n\\end{aligned}", detail: "environment" },
  { name: "\\begin{cases}", snippet: "\\begin{cases}\n  ${}\n\\end{cases}", detail: "environment" },
];

/** In math and in LaTeX environments. */
export const MATH_NAMES: LatexName[] = [...SYMBOLS, ...MATH];

/** In the text: the commands that make the structure, and the environments. */
export const TEXT_NAMES: LatexName[] = [
  args("cite", "${}"),
  args("citep", "${}"),
  args("citet", "${}"),
  args("citeauthor", "${}"),
  args("citeyear", "${}"),
  args("ref", "${}"),
  args("eqref", "${}"),
  args("label", "${}"),
  args("emph", "${}"),
  args("textbf", "${}"),
  args("textit", "${}"),
  ...["equation", "align", "figure", "table"].map(
    (env): LatexName => ({ name: `\\begin{${env}}`, snippet: `\\begin{${env}}\n  \${}\n\\end{${env}}`, detail: "environment" }),
  ),
];

const env = (name: string, body = "${}"): LatexName => ({
  name: `\\begin{${name}}`,
  snippet: `\\begin{${name}}\n  ${body}\n\\end{${name}}`,
  detail: "environment",
});

/** The commands of the document: the structure, the text, the references, the packages. */
const DOCUMENT: LatexName[] = [
  args("documentclass", "${}"),
  args("usepackage", "${}"),
  args("input", "${}"),
  args("include", "${}"),
  args("includegraphics", "${}"),
  args("title", "${}"),
  args("author", "${}"),
  args("date", "${}"),
  args("part", "${}"),
  args("chapter", "${}"),
  args("section", "${}"),
  args("subsection", "${}"),
  args("subsubsection", "${}"),
  args("paragraph", "${}"),
  args("caption", "${}"),
  args("footnote", "${}"),
  args("texttt", "${}"),
  args("textsc", "${}"),
  args("textrm", "${}"),
  args("underline", "${}"),
  args("url", "${}"),
  args("href", "${}", "${}"),
  args("newcommand", "\\\\${}", "${}"),
  args("renewcommand", "\\\\${}", "${}"),
  args("bibliography", "${}"),
  args("bibliographystyle", "${}"),
  args("addbibresource", "${}"),
  args("parencite", "${}"),
  args("textcite", "${}"),
  args("autoref", "${}"),
  args("cref", "${}"),
  args("pageref", "${}"),
  ...plain(
    ..."maketitle tableofcontents appendix newpage clearpage centering noindent item hline toprule midrule bottomrule printbibliography today".split(" "),
    ..."tiny scriptsize footnotesize small normalsize large Large LARGE huge Huge bfseries itshape ttfamily".split(" "),
    ..."textwidth linewidth columnwidth vspace hspace par ldots dots".split(" "),
  ),
];

/** The environments of the document. */
const ENVIRONMENTS: LatexName[] = [
  env("document"),
  env("abstract"),
  env("itemize", "\\item ${}"),
  env("enumerate", "\\item ${}"),
  env("description", "\\item[${}] "),
  env("equation"),
  env("equation*"),
  env("align"),
  env("align*"),
  env("gather"),
  env("multline"),
  env("figure", "\\centering\n  \\includegraphics[width=\\linewidth]{${}}\n  \\caption{}\n  \\label{fig:}"),
  env("table", "\\centering\n  \\caption{}\n  \\label{tab:}\n  \\begin{tabular}{${}}\n  \\end{tabular}"),
  env("tabular", "${}"),
  env("quote"),
  env("verbatim"),
  env("center"),
  env("minipage"),
  env("thebibliography"),
];

/** In a LaTeX file, anywhere after a backslash: all the names, those of the environments last. */
export const TEX_NAMES: LatexName[] = (() => {
  const seen = new Set<string>();
  return [...DOCUMENT, ...TEXT_NAMES, ...MATH_NAMES, ...ENVIRONMENTS].filter((n) => !seen.has(n.name) && !!seen.add(n.name));
})();
