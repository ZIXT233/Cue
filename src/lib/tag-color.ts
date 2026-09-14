export type TagColorStyle = {
  "--tag-hue": string;
  backgroundColor: string;
  color: string;
};

/** Presentation only: stable name-derived colors, independent of locale/order. */
export function tagColor(name: string): TagColorStyle {
  let hash = 2166136261;
  for (const character of name) hash = Math.imul(hash ^ character.codePointAt(0)!, 16777619);
  const hue = (hash >>> 0) % 360;
  return {
    "--tag-hue": String(hue),
    backgroundColor: `hsl(${hue} 65% 94%)`,
    color: `hsl(${hue} 42% 30%)`,
  };
}
