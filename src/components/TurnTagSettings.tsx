"use client";
import { ConfigDetailStack, ConfigDetailHeader, ConfigDetailHeaderInfo, ConfigDetailActions, ConfigFooter, ConfigPanelShell, ConfigDetail, ConfigDetailTitle, ConfigButton } from "./SettingsUi";
import { useI18n } from "@/hooks/useI18n";
import { useEffect, useState } from "react";
import { URGENT_CALL_TAG, isReservedUrgentName } from "@/lib/urgent-call";
import { DEFAULT_TURN_TAGS, type TurnTag } from "@/lib/turn-priority";
export function TurnTagSettings() {
  const { t } = useI18n();
  const [enabled, setEnabled] = useState(false);
  const [tags, setTags] = useState<TurnTag[]>([]);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [expandedTag, setExpandedTag] = useState<number | "builtin" | null>(null);
  useEffect(() => {
    void fetch("/api/card-queue").then(async r => {
      const data = await r.json(); if (!r.ok) throw new Error(data.error);
      setEnabled(data.turnTagsEnabled === true);
      setTags((data.turnTagDefinitions ?? DEFAULT_TURN_TAGS).filter((tag: TurnTag) => !isReservedUrgentName(tag.name))); setLoaded(true);
    }).catch(e => setError(String(e)));
  }, []);
  const toggle = async (next: boolean) => {
    setBusy(true); setError(""); setSaved(false);
    try {
      const response = await fetch("/api/card-queue", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ action: "turn_tags_enabled", enabled: next }) });
      const data = await response.json();
      if (!response.ok) throw new Error(data.error);
      setEnabled(data.turnTagsEnabled === true); setSaved(true);
    } catch (cause) { setError(String(cause)); } finally { setBusy(false); }
  };
  const save = async () => {
    setBusy(true); setError(""); setSaved(false);
    try {
      const r = await fetch("/api/card-queue", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ action: "turn_tags", tags }) });
      const data = await r.json(); if (!r.ok) throw new Error(data.error);
      setSaved(true);
    } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  return <ConfigPanelShell embedded title={t("queue.Turn Tags")} onClose={() => {}}><ConfigDetail><ConfigDetailStack className="turn-tag-settings">
    <ConfigDetailHeader>
      <ConfigDetailHeaderInfo><ConfigDetailTitle>{t("queue.Turn Tags")}</ConfigDetailTitle></ConfigDetailHeaderInfo>
      <ConfigDetailActions><ConfigButton size="small" disabled={!loaded || busy} onClick={() => { setSaved(false); setTags([...tags,{name:"",weight:0,description:""}]); }}>{t("queue.新增 Tag")}</ConfigButton></ConfigDetailActions>
    </ConfigDetailHeader>
    <label className="turn-tag-help"><input type="checkbox" checked={enabled} disabled={!loaded || busy} onChange={event => void toggle(event.target.checked)} /> {t("queue.tagsEnabled")}</label>
    <p className="turn-tag-help">{t("queue.tagsToggleHelp")}</p>
    <p className="turn-tag-help">{t("queue.模型只判断标签；权重由系统相加。修改后的规则用于下一次评估，不继承到下一轮。")}</p>
    <div className="turn-tag-list">
    <div className="turn-tag-columns" aria-hidden="true"><span>{t("queue.名称")}</span><span>{t("queue.权重")}</span><span>{t("queue.判断规则")}</span><span /></div>
    <div className="turn-tag-item turn-tag-builtin">
      <div className="turn-tag-row">
        <strong className="turn-tag-builtin-name" title={URGENT_CALL_TAG.name}>{URGENT_CALL_TAG.name}</strong>
        <strong className="turn-tag-builtin-weight" title={t("queue.urgentPriority")}>∞</strong>
        <button className="turn-tag-rule-toggle" type="button" aria-expanded={expandedTag === "builtin"} title={URGENT_CALL_TAG.description} onClick={() => setExpandedTag(expandedTag === "builtin" ? null : "builtin")}><span>{URGENT_CALL_TAG.description}</span></button>
        <span className="turn-tag-lock" title={t("queue.urgentFixed")} aria-label={t("queue.urgentFixed")}>🔒</span>
      </div>
      {expandedTag === "builtin" && <div className="turn-tag-rule-editor"><p>{URGENT_CALL_TAG.description}</p></div>}
    </div>
    {tags.map((tag, i) => <div className="turn-tag-item" key={i}>
      <div className="turn-tag-row">
        <input aria-label={t("queue.名称")} placeholder={t("queue.Tag 名称提示")} value={tag.name} disabled={busy} onChange={e => { setSaved(false); setTags(tags.map((item,j) => j === i ? {...item,name:e.target.value} : item)); }} />
        <input aria-label={t("queue.权重")} type="number" value={Number.isNaN(tag.weight) ? "" : tag.weight} disabled={busy} onChange={e => { setSaved(false); setTags(tags.map((item,j) => j === i ? {...item,weight:e.target.valueAsNumber} : item)); }} />
        <button className="turn-tag-rule-toggle" type="button" aria-expanded={expandedTag === i} title={tag.description} onClick={() => setExpandedTag(expandedTag === i ? null : i)}><span>{tag.description || t("queue.判断规则")}</span></button>
        <ConfigButton variant="ghost" size="small" disabled={busy} aria-label={t("queue.删除") + " " + tag.name} onClick={() => { setSaved(false); setExpandedTag(null); setTags(tags.filter((_,j) => j !== i)); }}>×</ConfigButton>
      </div>
      {expandedTag === i && <div className="turn-tag-rule-editor"><textarea autoFocus aria-label={t("queue.判断规则")} disabled={busy} value={tag.description} onChange={e => { setSaved(false); setTags(tags.map((item,j) => j === i ? {...item,description:e.target.value} : item)); }} /></div>}
    </div>)}
    </div>
  </ConfigDetailStack></ConfigDetail>
  <ConfigFooter status={error ? <span role="alert" className="turn-tag-error">{error}</span> : saved ? <span role="status">{t("queue.已保存")}</span> : null}>
    <ConfigButton variant="primary" disabled={!loaded || busy} onClick={() => void save()}>{t("queue.保存 Turn Tags")}</ConfigButton>
  </ConfigFooter></ConfigPanelShell>;
}
