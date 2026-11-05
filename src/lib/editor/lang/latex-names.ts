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
