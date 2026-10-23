// A small, safe Markdown renderer (no HTML injection): headings, paragraphs,
// lists, task lists, code blocks, tables, block quotes, rules, and inline
// code/bold/italic/links. Enough for project briefs and notes.
import { Fragment, type ReactNode } from "react";
import { openExternal } from "../bridge";

function inline(text: string, keyPrefix: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  const pattern = /(`[^`]+`)|(\*\*[^*]+\*\*)|(\*[^*\s][^*]*\*|_[^_\s][^_]*_)|(\[[^\]]+\]\([^)\s]+\))/g;
  let last = 0;
  let match: RegExpExecArray | null;
  let index = 0;
  while ((match = pattern.exec(text))) {
    if (match.index > last) nodes.push(text.slice(last, match.index));
    const token = match[0];
    const key = `${keyPrefix}-${index++}`;
    if (match[1]) nodes.push(<code key={key} className="rounded bg-muted px-1 py-0.5 font-mono text-[0.85em]">{token.slice(1, -1)}</code>);
    else if (match[2]) nodes.push(<strong key={key} className="font-semibold">{inline(token.slice(2, -2), key)}</strong>);
    else if (match[3]) nodes.push(<em key={key}>{inline(token.slice(1, -1), key)}</em>);
    else if (match[4]) {
      const [, label, href] = /^\[([^\]]+)\]\(([^)\s]+)\)$/.exec(token) ?? [];
      const external = /^https?:\/\//i.test(href ?? "");
      nodes.push(external
        ? <a key={key} href={href} className="text-primary underline-offset-2 hover:underline" onClick={(event) => { event.preventDefault(); void openExternal(href); }}>{label}</a>
        : <span key={key} className="text-primary" title={href}>{label}</span>);
    }
    last = match.index + token.length;
  }
  if (last < text.length) nodes.push(text.slice(last));
  return nodes;
}

const isTableRow = (line: string) => /^\s*\|.*\|\s*$/.test(line);
const cells = (line: string) => line.trim().replace(/^\||\|$/g, "").split("|").map((cell) => cell.trim());

export function Markdown({ source, className }: { source: string; className?: string }) {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const blocks: ReactNode[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    const key = `b${i}`;
    if (!line.trim()) { i++; continue; }
    const fence = /^```(\w*)/.exec(line);
    if (fence) {
      const body: string[] = [];
      i++;
      while (i < lines.length && !lines[i].startsWith("```")) body.push(lines[i++]);
      i++;
      blocks.push(<pre key={key} className="overflow-x-auto rounded-md border border-border bg-muted/40 p-3 font-mono text-xs leading-relaxed">{body.join("\n")}</pre>);
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      const level = heading[1].length;
      const classes = ["", "text-xl font-semibold", "mt-2 border-b border-border pb-1 text-lg font-semibold", "text-base font-semibold", "text-sm font-semibold", "text-sm font-semibold", "text-sm font-medium text-muted-foreground"];
      const Tag = `h${Math.min(level + 1, 6)}` as "h2";
      blocks.push(<Tag key={key} className={classes[level]}>{inline(heading[2], key)}</Tag>);
      i++;
      continue;
    }
    if (/^(-{3,}|\*{3,}|_{3,})\s*$/.test(line)) { blocks.push(<hr key={key} className="border-border" />); i++; continue; }
    if (isTableRow(line) && i + 1 < lines.length && /^\s*\|?\s*:?-{2,}/.test(lines[i + 1])) {
      const header = cells(line);
      i += 2;
      const rows: string[][] = [];
      while (i < lines.length && isTableRow(lines[i])) rows.push(cells(lines[i++]));
      blocks.push(
        <div key={key} className="overflow-x-auto"><table className="w-full border-collapse text-sm">
          <thead><tr>{header.map((cell, c) => <th key={c} className="border-b border-border px-2 py-1 text-left font-medium text-muted-foreground">{inline(cell, `${key}h${c}`)}</th>)}</tr></thead>
          <tbody>{rows.map((row, r) => <tr key={r} className="border-b border-border/60">{row.map((cell, c) => <td key={c} className="px-2 py-1 align-top">{inline(cell, `${key}r${r}c${c}`)}</td>)}</tr>)}</tbody>
        </table></div>,
      );
      continue;
    }
    if (/^>\s?/.test(line)) {
      const body: string[] = [];
      while (i < lines.length && /^>\s?/.test(lines[i])) body.push(lines[i++].replace(/^>\s?/, ""));
      blocks.push(<blockquote key={key} className="border-l-2 border-primary/50 pl-3 text-muted-foreground">{inline(body.join(" "), key)}</blockquote>);
      continue;
    }
    const listItem = /^(\s*)([-*+]|\d+\.)\s+(.*)$/;
    if (listItem.test(line)) {
      const ordered = /^\s*\d+\./.test(line);
      const items: Array<{ depth: number; text: string }> = [];
      while (i < lines.length && listItem.test(lines[i])) {
        const [, indent, , text] = listItem.exec(lines[i]) ?? [];
        items.push({ depth: Math.floor((indent?.length ?? 0) / 2), text: text ?? "" });
        i++;
      }
      const List = ordered ? "ol" : "ul";
      blocks.push(
        <List key={key} className={ordered ? "list-decimal space-y-0.5 pl-5" : "list-disc space-y-0.5 pl-5"}>
          {items.map((item, n) => {
            const task = /^\[( |x|X)\]\s+(.*)$/.exec(item.text);
            return (
              <li key={n} style={{ marginLeft: item.depth * 16 }} className={task ? "list-none -ml-5" : undefined}>
                {task ? <span className="inline-flex items-center gap-2"><input type="checkbox" readOnly checked={task[1] !== " "} className="size-3.5 accent-[var(--primary)]" />{inline(task[2], `${key}-${n}`)}</span> : inline(item.text, `${key}-${n}`)}
              </li>
            );
          })}
        </List>,
      );
      continue;
    }
    const paragraph: string[] = [];
    while (i < lines.length && lines[i].trim() && !/^(#{1,6}\s|```|>|\s*([-*+]|\d+\.)\s)/.test(lines[i]) && !isTableRow(lines[i])) paragraph.push(lines[i++].trim());
    if (!paragraph.length) { paragraph.push(line); i++; }
    blocks.push(<p key={key} className="leading-relaxed">{paragraph.map((text, n) => <Fragment key={n}>{n ? " " : null}{inline(text, `${key}-${n}`)}</Fragment>)}</p>);
  }
  return <div className={className ?? "space-y-3 text-sm"}>{blocks}</div>;
}
