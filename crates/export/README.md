# pdfcraft-export

Layer L4 writers for Word (DOCX), HTML, Rich Text Format (RTF), and PowerPoint (PPTX).
The engine supplies each page's text blocks, images, dimensions, and background.
The writers infer paragraphs, headings, and tables from that content.

RTF preserves text size, bold, italic, color, detected tables, and page breaks.
Hard line endings inside text are written as `\line` controls, preserving their paragraph or table cell.
LF and CR each produce one hard break, and a CRLF pair produces one break.
Repeated, leading, and trailing line endings are retained by the text encoder.
RTF syntax characters and Unicode text keep their existing escaping.

RTF leaves images out, and layout and reading-order fidelity remain limited by the inferred blocks.
This hard-break handling applies to RTF; the other writers retain their existing behavior.
