import { useQuery } from "@tanstack/react-query";
import { Copy, Eye, EyeOff, FileUp, Trash2, Wand2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { StatusBadge } from "@/components/ds/status-badge";
import { Button } from "@/components/ui/button";
import { toAppError } from "@/lib/errors";
import { api } from "@/lib/ipc";
import {
  parseRejection,
  validateFile,
  validateText,
  type FieldStatus,
  type Manifest,
  type ManifestField,
  type Rejection,
} from "@/lib/integrations";
import { toastError } from "@/lib/toast";
import { cn } from "@/lib/utils";
import { inputCls } from "../sections/shared";
import { ConfirmDialog } from "./confirm-dialog";
import { GuideSlot } from "./guide-slot";

const HIDE_AFTER_S = 30;

function toBase64(bytes: Uint8Array): string {
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}

type Confirm = null | "show" | "copy" | "replace" | "delete";

export function SecretField({
  projectId,
  manifest,
  field,
  status,
  highlighted,
  onChanged,
}: {
  projectId: string;
  manifest: Manifest;
  field: ManifestField;
  status: FieldStatus;
  highlighted: boolean;
  onChanged: () => Promise<unknown>;
}) {
  const { t } = useTranslation();
  const isFile = field.type === "file";
  const id = `secret-${manifest.id}-${field.id}`;
  const [draft, setDraft] = useState("");
  const [editing, setEditing] = useState(false);
  const [error, setError] = useState<Rejection | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState<Confirm>(null);
  const [shown, setShown] = useState<{ value: string; left: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const [pendingFile, setPendingFile] = useState<{ name: string; data: Uint8Array } | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);

  // Publieke waarden mogen gewoon zichtbaar zijn; geheime blijven verborgen tot de gebruiker erom vraagt.
  const publicValue = useQuery({
    queryKey: ["publicValue", projectId, manifest.id, field.id, status.setAt],
    queryFn: () => api.secretFieldReveal(projectId, manifest.id, field.id),
    enabled: status.present && !field.geheim && !isFile,
  });

  // Een getoonde waarde verdwijnt na 30 seconden vanzelf.
  useEffect(() => {
    if (!shown) return;
    const tick = setInterval(() => setShown((s) => (s && s.left > 1 ? { ...s, left: s.left - 1 } : null)), 1000);
    return () => clearInterval(tick);
  }, [shown === null]); // eslint-disable-line react-hooks/exhaustive-deps

  const fail = (e: unknown) => {
    const err = toAppError(e);
    if (err.code === "rejected") setError(parseRejection(err.message));
    else toastError(e, t);
  };
  const errorText = (r: Rejection) =>
    t(`sec.secrets.errors.${r.code}`, { detail: r.detail ?? "", defaultValue: t("sec.secrets.errors.generic") });

  const saved = async (msg: string) => {
    setDraft("");
    setEditing(false);
    setPendingFile(null);
    setError(null);
    setShown(null);
    await onChanged();
    toast.success(msg);
  };

  const saveText = async () => {
    const r = validateText(field, draft);
    if (r) return setError(r);
    setBusy(true);
    try {
      await api.secretFieldSetText(projectId, manifest.id, field.id, draft);
      await saved(t("sec.secrets.saved", { name: t(field.label) }));
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
      setConfirm(null);
    }
  };

  const uploadFile = async (name: string, data: Uint8Array) => {
    setBusy(true);
    try {
      const res = await api.secretFieldSetFile(projectId, manifest.id, field.id, name, toBase64(data));
      const who = res.derived.client_email ?? res.derived.project_id;
      await saved(
        who
          ? t("sec.secrets.savedFileWho", { name: t(field.label), who })
          : t("sec.secrets.saved", { name: t(field.label) }),
      );
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
      setConfirm(null);
    }
  };

  const takeFile = async (file: File | undefined) => {
    if (!file) return;
    const data = new Uint8Array(await file.arrayBuffer());
    let text: string | null;
    try {
      text = new TextDecoder("utf-8", { fatal: true }).decode(data);
    } catch {
      text = null;
    }
    const check = validateFile(field, file.name, text, data.length);
    if (check.rejection) return setError(check.rejection);
    setError(null);
    if (status.present) {
      setPendingFile({ name: file.name, data });
      setConfirm("replace");
    } else await uploadFile(file.name, data);
  };

  const reveal = async (then: "show" | "copy") => {
    setBusy(true);
    try {
      const value = await api.secretFieldReveal(projectId, manifest.id, field.id);
      if (then === "show") setShown({ value, left: HIDE_AFTER_S });
      else {
        await navigator.clipboard.writeText(value);
        toast.success(t("sec.secrets.copied"));
      }
    } catch (e) {
      toastError(e, t);
    } finally {
      setBusy(false);
      setConfirm(null);
    }
  };

  const remove = async () => {
    setBusy(true);
    try {
      await api.secretFieldDelete(projectId, manifest.id, field.id);
      setShown(null);
      await onChanged();
      toast.success(t("sec.secrets.deleted", { name: t(field.label) }));
    } catch (e) {
      toastError(e, t);
    } finally {
      setBusy(false);
      setConfirm(null);
    }
  };

  const [owner, setOwner] = useState("");
  const [org, setOrg] = useState("");
  const [generating, setGenerating] = useState(false);
  const canGenerate = manifest.acties?.includes("keystore_genereren") && field.id === "keystore";
  const generate = async () => {
    setBusy(true);
    try {
      await api.signingCreateKeystore(projectId, owner || t("sec.secrets.generate.defaultOwner"), org, "NL");
      setGenerating(false);
      await onChanged();
      toast.success(t("sec.secrets.generate.done"));
    } catch (e) {
      toastError(e, t);
    } finally {
      setBusy(false);
    }
  };

  const showInput = !status.present || editing;
  const label = t(field.label);
  const helpId = `${id}-help`;
  const errId = `${id}-err`;

  return (
    <div
      id={id}
      tabIndex={-1}
      className={cn(
        "space-y-2 rounded-lg border p-3 transition-shadow outline-none",
        highlighted && "ring-primary ring-2",
      )}
    >
      <div className="flex flex-wrap items-center gap-2">
        <label htmlFor={`${id}-input`} className="t-body font-medium">
          {label}
          {field.verplicht ? (
            <span className="text-destructive"> *</span>
          ) : (
            <span className="text-muted-foreground t-small"> {t("sec.secrets.optional")}</span>
          )}
        </label>
        <StatusBadge tone={field.geheim ? "warning" : "info"}>
          {field.geheim ? t("sec.secrets.secret") : t("sec.secrets.public")}
        </StatusBadge>
        <span className="t-small text-muted-foreground">
          {field.scope === "global" ? t("sec.secrets.scope.globalShort") : t("sec.secrets.scope.projectShort")}
        </span>
        <span className="ml-auto" />
        <GuideSlot guideId={field.guideId} />
      </div>
      <p id={helpId} className="t-small text-muted-foreground">
        {t(field.hulptekst)}
      </p>

      {!showInput && !isFile && (
        <div className="flex flex-wrap items-center gap-2">
          <code
            className="bg-muted t-small max-w-full min-w-0 truncate rounded-md px-2 py-1 font-mono"
            aria-label={t("sec.secrets.valueSaved")}
          >
            {field.geheim ? (shown ? shown.value : "••••••••••••") : (publicValue.data ?? "…")}
          </code>
          {field.geheim && (
            <>
              {shown ? (
                <Button size="xs" variant="outline" onClick={() => setShown(null)}>
                  <EyeOff /> {t("sec.secrets.hide", { s: shown.left })}
                </Button>
              ) : (
                <Button size="xs" variant="outline" disabled={busy} onClick={() => setConfirm("show")}>
                  <Eye /> {t("sec.secrets.show")}
                </Button>
              )}
              <Button size="xs" variant="outline" disabled={busy} onClick={() => setConfirm("copy")}>
                <Copy /> {t("sec.secrets.copy")}
              </Button>
            </>
          )}
          <Button size="xs" variant="outline" disabled={busy} onClick={() => setEditing(true)}>
            {t("sec.secrets.replace")}
          </Button>
          <Button size="xs" variant="ghost" disabled={busy} onClick={() => setConfirm("delete")}>
            <Trash2 /> {t("sec.secrets.delete")}
          </Button>
        </div>
      )}

      {!showInput && isFile && (
        <div className="flex flex-wrap items-center gap-2">
          <span className="t-body">{t("sec.secrets.fileSaved", { name: status.fileName ?? t(field.label) })}</span>
          {Object.entries(status.derived).map(([k, v]) => (
            <span key={k} className="t-small text-muted-foreground">
              {t(`sec.secrets.derived.${k}`, { defaultValue: k })}: <span className="font-mono">{v}</span>
            </span>
          ))}
          <Button size="xs" variant="outline" disabled={busy} onClick={() => setEditing(true)}>
            {t("sec.secrets.replace")}
          </Button>
          <Button size="xs" variant="ghost" disabled={busy} onClick={() => setConfirm("delete")}>
            <Trash2 /> {t("sec.secrets.delete")}
          </Button>
        </div>
      )}

      {showInput && !isFile && (
        <form
          className="flex flex-wrap gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (status.present) setConfirm("replace");
            else void saveText();
          }}
        >
          {field.type === "select" ? (
            <select
              id={`${id}-input`}
              className={cn(inputCls, "max-w-xs")}
              value={draft}
              aria-describedby={helpId}
              onChange={(e) => setDraft(e.target.value)}
            >
              <option value="">{t("sec.secrets.choose")}</option>
              {(field.opties ?? []).map((o) => (
                <option key={o} value={o}>
                  {o}
                </option>
              ))}
            </select>
          ) : (
            <input
              id={`${id}-input`}
              type={field.geheim ? "password" : "text"}
              autoComplete="off"
              spellCheck={false}
              value={draft}
              placeholder={field.standaard ?? undefined}
              aria-invalid={error ? true : undefined}
              aria-describedby={error ? `${helpId} ${errId}` : helpId}
              onChange={(e) => {
                const v = e.target.value;
                setDraft(v);
                setError(v.trim() ? validateText(field, v) : null);
              }}
              className={cn(inputCls, "min-w-0 flex-1 font-mono")}
            />
          )}
          <Button type="submit" disabled={busy || !draft.trim()}>
            {t("sec.secrets.save")}
          </Button>
          {status.present && (
            <Button
              type="button"
              variant="ghost"
              onClick={() => {
                setEditing(false);
                setDraft("");
                setError(null);
              }}
            >
              {t("common.cancel")}
            </Button>
          )}
        </form>
      )}

      {showInput && isFile && (
        <div className="space-y-2">
          <div
            role="group"
            aria-label={t("sec.secrets.dropLabel", { name: label })}
            onDragOver={(e) => {
              e.preventDefault();
              setDragging(true);
            }}
            onDragLeave={() => setDragging(false)}
            onDrop={(e) => {
              e.preventDefault();
              setDragging(false);
              void takeFile(e.dataTransfer.files[0]);
            }}
            className={cn(
              "flex flex-col items-center gap-2 rounded-lg border-2 border-dashed p-4 text-center transition-colors",
              dragging ? "border-primary bg-accent/50" : "border-border",
            )}
          >
            <FileUp aria-hidden className="text-muted-foreground size-5" />
            <p className="t-small">
              {t("sec.secrets.dropHere", {
                types: (field.validatie.bestandstype ?? []).map((x) => `.${x}`).join(", "),
              })}
            </p>
            <input
              ref={fileInput}
              id={`${id}-input`}
              type="file"
              className="sr-only"
              accept={(field.validatie.bestandstype ?? []).map((x) => `.${x}`).join(",")}
              onChange={(e) => {
                void takeFile(e.target.files?.[0]);
                e.target.value = "";
              }}
              aria-describedby={helpId}
            />
            <div className="flex flex-wrap justify-center gap-2">
              <Button size="sm" variant="outline" disabled={busy} onClick={() => fileInput.current?.click()}>
                {t("sec.secrets.browse")}
              </Button>
              {canGenerate && !status.present && (
                <Button size="sm" disabled={busy} onClick={() => setGenerating((g) => !g)}>
                  <Wand2 /> {t("sec.secrets.generate.button")}
                </Button>
              )}
              {status.present && (
                <Button size="sm" variant="ghost" onClick={() => setEditing(false)}>
                  {t("common.cancel")}
                </Button>
              )}
            </div>
          </div>
          {generating && (
            <form
              className="bg-muted/50 space-y-2 rounded-lg p-3"
              onSubmit={(e) => {
                e.preventDefault();
                void generate();
              }}
            >
              <p className="t-small">{t("sec.secrets.generate.intro")}</p>
              <div className="grid gap-2 sm:grid-cols-2">
                <input
                  aria-label={t("sec.secrets.generate.owner")}
                  placeholder={t("sec.secrets.generate.owner")}
                  className={inputCls}
                  value={owner}
                  onChange={(e) => setOwner(e.target.value)}
                />
                <input
                  aria-label={t("sec.secrets.generate.org")}
                  placeholder={t("sec.secrets.generate.org")}
                  className={inputCls}
                  value={org}
                  onChange={(e) => setOrg(e.target.value)}
                />
              </div>
              <Button type="submit" size="sm" disabled={busy}>
                {t("sec.secrets.generate.create")}
              </Button>
            </form>
          )}
        </div>
      )}

      {error && (
        <p id={errId} role="alert" className="text-destructive t-small">
          {errorText(error)}
        </p>
      )}

      <ConfirmDialog
        open={confirm === "show"}
        title={t("sec.secrets.confirm.showTitle")}
        body={t("sec.secrets.confirm.showBody", { name: label, s: HIDE_AFTER_S })}
        confirmLabel={t("sec.secrets.show")}
        busy={busy}
        onConfirm={() => void reveal("show")}
        onCancel={() => setConfirm(null)}
      />
      <ConfirmDialog
        open={confirm === "copy"}
        title={t("sec.secrets.confirm.copyTitle")}
        body={t("sec.secrets.confirm.copyBody", { name: label })}
        confirmLabel={t("sec.secrets.copy")}
        busy={busy}
        onConfirm={() => void reveal("copy")}
        onCancel={() => setConfirm(null)}
      />
      <ConfirmDialog
        open={confirm === "replace"}
        title={t("sec.secrets.confirm.replaceTitle", { name: label })}
        body={t(
          manifest.id === "android_signing" && field.id === "keystore"
            ? "sec.secrets.confirm.replaceKeystore"
            : "sec.secrets.confirm.replaceBody",
        )}
        confirmLabel={t("sec.secrets.replace")}
        destructive
        busy={busy}
        onConfirm={() => (pendingFile ? void uploadFile(pendingFile.name, pendingFile.data) : void saveText())}
        onCancel={() => {
          setConfirm(null);
          setPendingFile(null);
        }}
      />
      <ConfirmDialog
        open={confirm === "delete"}
        title={t("sec.secrets.confirm.deleteTitle", { name: label })}
        body={t(
          manifest.id === "android_signing" && field.id === "keystore"
            ? "sec.secrets.confirm.deleteKeystore"
            : "sec.secrets.confirm.deleteBody",
        )}
        confirmLabel={t("sec.secrets.delete")}
        destructive
        busy={busy}
        onConfirm={() => void remove()}
        onCancel={() => setConfirm(null)}
      />
    </div>
  );
}
