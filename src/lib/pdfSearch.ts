// Show in PDF, until a build gives an exact map (SyncTeX): the words around the cursor looked for in the PDF. The line
// is taken as the PDF has it, its markup out (Markdown emphasis, links, citations, cross-references, math, LaTeX
// commands, CriticMarkup), and a few phrases of it are offered, the longest around the cursor first, for the viewer's
// find to try in turn.

/** The text of a source line as it reads in the PDF, roughly: what does not show (or shows otherwise) taken out. */
export function plainOf(line: string): string {
  return (
    line
      // CriticMarkup: the new text of a suggestion, no deletions, no notes
      .replace(/\{--[\s\S]*?--\}|\{>>[\s\S]*?<<\}/g, " ")
      .replace(/\{~~[\s\S]*?~>([\s\S]*?)~~\}/g, "$1")
      .replace(/\{\+\+|\+\+\}|\{==|==\}/g, "")
      // math and code: shown otherwise
      .replace(/\$\$[\s\S]*?\$\$|\$[^$]*\$|`[^`]*`/g, " ")
      // citations and cross-references: numbers or names in the PDF
      .replace(/\[[^\]]*@[^\]]*\]|@\w[\w:.-]*/g, " ")
      // links and images: their text
      .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
      // LaTeX: citations, references and labels go whole; another command's argument stays, the command goes
      .replace(/\\(cite\w*|ref|eqref|autoref|[cC]ref|pageref|label)\*?(\[[^\]]*\])*\{[^}]*\}/g, " ")
      .replace(/\\[A-Za-z]+\*?(\[[^\]]*\])?\{([^}]*)\}/g, "$2")
      .replace(/\\[A-Za-z]+\*?/g, " ")
      // Markdown marks
      .replace(/^\s*(#+|[-*+]|\d+\.|>)\s+/, "")
      .replace(/[*_~{}]/g, "")
      .replace(/\s+/g, " ")
      .trim()
  );
}

/** Phrases of the line around `column` to look for in the PDF, the most telling first: six words around the cursor,
 *  then four, then three before it and three after it; none shorter than three words. */
export function phrasesAt(line: string, column: number): string[] {
  const before = plainOf(line.slice(0, column)).split(" ").filter(Boolean);
  const after = plainOf(line.slice(column)).split(" ").filter(Boolean);
  const words = [...before, ...after];
  const at = before.length;
  const around = (n: number) => {
    const from = Math.max(0, Math.min(at - Math.floor(n / 2), words.length - n));
    return words.slice(from, from + n);
  };
  const phrases = [around(6), around(4), before.slice(-3), after.slice(0, 3)]
    .filter((p) => p.length >= 3)
    .map((p) => p.join(" "));
  return [...new Set(phrases)];
}
