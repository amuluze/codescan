export function Badge({ html }) {
  // ruleid: jsx/no-dangerous-inner-html
  return <span dangerouslySetInnerHTML={{ __html: html }} />;
}

export function SafeBadge({ text }) {
  return <span title={text}>{text}</span>;
}
