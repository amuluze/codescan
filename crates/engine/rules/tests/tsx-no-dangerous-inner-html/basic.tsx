type Props = { html: string };
type TextProps = { text: string };

export function Badge({ html }: Props) {
  // ruleid: tsx/no-dangerous-inner-html
  return <span dangerouslySetInnerHTML={{ __html: html }} />;
}

export function SafeBadge({ text }: TextProps) {
  return <span title={text}>{text}</span>;
}
