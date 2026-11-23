import {
  ArrowRightLeft,
  AudioLines,
  Check,
  ChevronLeft,
  ChevronRight,
  CircleAlert,
  Clapperboard,
  Download,
  Film,
  FolderOpen,
  Image,
  SlidersHorizontal,
  X,
} from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { IconButton, TextButton } from "../../components/Button";
import { Ring } from "../../components/Ring";
import { useIsland } from "../../island/context";
import { useNow } from "../../lib/useNow";
import type { Activity, DuriApp, IconProps } from "../types";
import {
  commonTargets,
  convertActions,
  formatBytes,
  formatClock,
  parseClock,
  secondsLeft,
  startConvert,
  usable,
  useConvert,
  useFresh,
  useStaged,
  type Advanced,
  type Job,
  type Kind,
  type Quality,
  type Staged,
  type Target,
  type Tools,
} from "./store";
import styles from "./convert.module.css";

const ACCENT = "var(--accent-convert)";
const GREY = "var(--island-fg)";

function KindIcon({ kind, size = 16 }: { kind: Kind | null | undefined; size?: number }) {
  switch (kind) {
    case "video":
      return <Film size={size} />;
    case "audio":
      return <AudioLines size={size} />;
    case "image":
      return <Image size={size} />;
    case "animation":
      return <Clapperboard size={size} />;
    default:
      return <ArrowRightLeft size={size} />;
  }
}

const ext = (id: string) => id.toUpperCase();

/** The job the island talks about: the one running, else the next queued. */
function currentJob(jobs: Job[]): Job | undefined {
  return jobs.find((j) => j.status === "running") ?? jobs.find((j) => j.status === "queued");
}

function useLatestFresh(): Job | undefined {
  const { jobs } = useConvert();
  const fresh = useFresh();
  const last = fresh[fresh.length - 1];
  return last === undefined ? undefined : jobs.find((j) => j.id === last);
}

// ---------- drop target ----------

/** What the island turns into while files are dragged over it. */
function Drop({ paths }: { paths: string[] }) {
  const { t } = useTranslation();
  return (
    <div className={styles.drop}>
      <span className={styles.dropRing} aria-hidden />
      <span className={styles.dropTile}>
        <ArrowRightLeft size={20} />
      </span>
      <div className={styles.meta}>
        <span className={styles.title}>{t("convert.dropTitle")}</span>
        <span className={styles.detail}>{t("convert.files", { count: paths.length })}</span>
      </div>
      {paths.length > 1 && (
        <span className={`${styles.badge} tabular`} aria-hidden>
          {paths.length}
        </span>
      )}
    </div>
  );
}

// ---------- pill, bubble, compact ----------

/** ffmpeg download progress (0…1), or null when it isn't downloading. */
function downloadProgress(tools: Tools): number | null {
  return tools.state === "downloading" ? tools.progress : null;
}

function Pill() {
  const { jobs, tools } = useConvert();
  const staged = useStaged();
  const done = useLatestFresh();
  const job = currentJob(jobs);
  const download = downloadProgress(tools);
  const progress = job?.progress ?? download ?? 0;
  return (
    <div className={styles.pill}>
      <Ring progress={job || download !== null ? progress : done ? 1 : 0} size={20} color={ACCENT}>
        {done?.status === "failed" ? (
          <CircleAlert size={11} color="var(--accent-stop)" />
        ) : download !== null && !job ? (
          <Download size={10} color={ACCENT} />
        ) : (
          <ArrowRightLeft size={10} color={ACCENT} />
        )}
      </Ring>
      <span className={`${styles.pillText} tabular`} style={{ color: ACCENT }}>
        {download !== null && !job ? (
          `${Math.floor(download * 100)}%`
        ) : job ? (
          job.progress === null ? "…" : `${Math.floor(progress * 100)}%`
        ) : done ? (
          done.status === "failed" ? <X size={15} color="var(--accent-stop)" /> : <Check size={16} />
        ) : staged ? (
          staged.paths.length
        ) : null}
      </span>
    </div>
  );
}

function Minimal() {
  const { jobs, tools } = useConvert();
  const job = currentJob(jobs);
  return (
    <Ring progress={job?.progress ?? downloadProgress(tools) ?? 0} size={22} color={ACCENT}>
      <ArrowRightLeft size={11} color={ACCENT} />
    </Ring>
  );
}

function Compact() {
  const { t } = useTranslation();
  const { jobs, tools, packageBytes } = useConvert();
  const staged = useStaged();
  const done = useLatestFresh();
  const job = currentJob(jobs);
  const now = useNow(!!job, 1000);
  const download = downloadProgress(tools);

  if (download !== null && !job && !done) {
    const size = formatBytes(packageBytes);
    return (
      <div className={styles.compact}>
        <Ring progress={download} size={44} stroke={4} color={ACCENT}>
          <Download size={16} color={ACCENT} />
        </Ring>
        <div className={styles.meta}>
          <span className={styles.title}>{t("convert.downloadingTitle")}</span>
          <span className={`${styles.detail} tabular`}>
            {`${Math.floor(download * 100)}% · ${t("convert.downloading", { done: formatBytes(packageBytes * download), size })}`}
          </span>
        </div>
        <IconButton label={t("convert.cancel")} size={36} variant="tinted" tone={GREY} onClick={convertActions.cancelInstall}>
          <X size={16} />
        </IconButton>
      </div>
    );
  }

  if (done && !job) {
    const failed = done.status === "failed";
    return (
      <div className={styles.compact} role="status">
        <span className={styles.tile} data-tone={failed ? "stop" : "accent"}>
          {failed ? <CircleAlert size={20} /> : <Check size={20} />}
        </span>
        <div className={styles.meta}>
          <span className={styles.title}>{failed ? t("convert.failed") : t("convert.converted")}</span>
          <span className={styles.detail}>
            {failed ? done.error : `${done.name} → ${ext(done.target)}`}
          </span>
        </div>
        {failed || !done.output ? (
          <TextButton variant="tinted" tone={GREY} onClick={convertActions.dismissFresh}>
            {t("convert.close")}
          </TextButton>
        ) : (
          <TextButton
            variant="filled"
            tone={ACCENT}
            onClick={() => {
              convertActions.reveal(done.output!);
              convertActions.dismissFresh();
            }}
          >
            {t("convert.showInFolder")}
          </TextButton>
        )}
      </div>
    );
  }

  if (job) {
    const left = secondsLeft(job, now);
    const waiting = jobs.filter((j) => j.status === "queued").length - (job.status === "queued" ? 1 : 0);
    const parts = [
      job.progress === null ? t("convert.working") : `${Math.floor((job.progress ?? 0) * 100)}%`,
      left !== null ? t("convert.left", { time: formatClock(left) }) : null,
      waiting > 0 ? t("convert.waiting", { count: waiting }) : null,
    ].filter(Boolean);
    return (
      <div className={styles.compact}>
        <Ring progress={job.progress ?? 0} size={44} stroke={4} color={ACCENT}>
          <ArrowRightLeft size={16} color={ACCENT} />
        </Ring>
        <div className={styles.meta}>
          <span className={styles.title}>
            {job.name} → {ext(job.target)}
          </span>
          <span className={`${styles.detail} tabular`}>{parts.join(" · ")}</span>
        </div>
        <IconButton label={t("convert.cancel")} size={36} variant="tinted" tone={GREY} onClick={() => convertActions.cancel(job.id)}>
          <X size={16} />
        </IconButton>
      </div>
    );
  }

  return (
    <div className={styles.compact}>
      <span className={styles.tile} data-tone="accent">
        <ArrowRightLeft size={20} />
      </span>
      <div className={styles.meta}>
        <span className={styles.title}>
          {staged ? t("convert.ready", { count: staged.paths.length }) : t("apps.convert.name")}
        </span>
        <span className={styles.detail}>{staged ? t("convert.pickFormat") : t("convert.dropHint")}</span>
      </div>
    </div>
  );
}

// ---------- expanded ----------

function Setup({ tools, bytes }: { tools: Tools; bytes: number }) {
  const { t } = useTranslation();
  const size = formatBytes(bytes);
  const downloading = tools.state === "downloading";
  const percent = downloading ? Math.floor(tools.progress * 100) : 0;
  return (
    <div className={styles.setup}>
      <span className={styles.bigTile}>
        <Download size={26} />
      </span>
      <h2 className={styles.setupTitle}>{tools.state === "failed" ? t("convert.setupFailed") : t("convert.setupTitle")}</h2>
      <p className={styles.setupText}>
        {tools.state === "failed" ? tools.error : t("convert.setupText", { size })}
      </p>
      {downloading ? (
        <div className={styles.setupProgress}>
          <div className={styles.bar} role="progressbar" aria-valuenow={percent} aria-valuemin={0} aria-valuemax={100}>
            <div className={styles.fill} style={{ width: `${Math.max(2, percent)}%` }} />
          </div>
          <span className={`${styles.detail} tabular`}>
            {t("convert.downloading", { done: formatBytes(bytes * tools.progress), size })}
          </span>
          <TextButton variant="tinted" tone={GREY} onClick={convertActions.cancelInstall}>
            {t("convert.cancel")}
          </TextButton>
        </div>
      ) : (
        <div className={styles.setupActions}>
          <TextButton variant="tinted" tone={GREY} onClick={convertActions.unstage}>
            {t("convert.notNow")}
          </TextButton>
          <TextButton variant="filled" tone={ACCENT} onClick={convertActions.install}>
            {tools.state === "failed" ? t("convert.retry") : t("convert.download", { size })}
          </TextButton>
        </div>
      )}
    </div>
  );
}

const QUALITIES: Quality[] = ["high", "balanced", "small"];
const GROUPS: Kind[] = ["video", "audio", "image", "animation"];

function Summary({ staged }: { staged: Staged }) {
  const { t } = useTranslation();
  const files = staged.files ?? [];
  const ok = usable(files);
  const skipped = files.length - ok.length;
  const one = files.length === 1 ? files[0] : null;
  let detail: string;
  if (!staged.files) detail = t("convert.looking");
  else if (one && !ok.length) detail = t("convert.unsupported");
  else if (one) {
    const m = one.media;
    detail = [
      m.width && m.height ? `${m.width}×${m.height}` : null,
      m.duration ? formatClock(m.duration) : null,
      formatBytes(one.bytes),
    ]
      .filter(Boolean)
      .join(" · ");
  } else {
    const counts = GROUPS.map((k) => [k, ok.filter((f) => f.media.kind === k).length] as const).filter(([, n]) => n);
    detail = [
      ...counts.map(([k, n]) => t(`convert.kinds.${k}`, { count: n })),
      skipped ? t("convert.skipped", { count: skipped }) : null,
    ]
      .filter(Boolean)
      .join(", ");
  }
  const kind = ok[0]?.media.kind;
  return (
    <div className={styles.summary}>
      <span className={styles.tile} data-tone="accent" data-size="small">
        <KindIcon kind={files.length === 1 || ok.every((f) => f.media.kind === kind) ? kind : null} size={17} />
      </span>
      <div className={styles.meta}>
        <span className={styles.title}>{one ? one.name : t("convert.files", { count: staged.paths.length })}</span>
        <span className={`${styles.detail} tabular`}>{detail}</span>
      </div>
      <IconButton label={t("convert.clearFiles")} size={30} variant="tinted" tone={GREY} onClick={convertActions.unstage}>
        <X size={15} />
      </IconButton>
    </div>
  );
}

function TargetChips({ targets, selected }: { targets: Target[]; selected: string | null }) {
  const { t } = useTranslation();
  const groups = GROUPS.map((k) => [k, targets.filter((x) => x.output === k)] as const).filter(([, l]) => l.length);
  return (
    <div className={styles.groups} role="radiogroup" aria-label={t("convert.format")}>
      {groups.map(([kind, list]) => (
        <div key={kind} className={styles.group}>
          <span className={styles.groupLabel}>{t(`convert.outputs.${kind}`)}</span>
          <div className={styles.chips}>
            {list.map((target) => (
              <button
                key={target.id}
                type="button"
                role="radio"
                aria-checked={selected === target.id}
                className={styles.chip}
                onClick={(e) => {
                  e.stopPropagation();
                  convertActions.setTarget(target.id);
                }}
              >
                {ext(target.id)}
              </button>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

interface ChoiceProps<T> {
  label: string;
  values: T[];
  value: T;
  format: (v: T) => string;
  onChange: (v: T) => void;
}

/** "‹ 1080p ›": steps through a short list, like the Timer's custom length. */
function Choice<T>({ label, values, value, format, onChange }: ChoiceProps<T>) {
  const { t } = useTranslation();
  const i = Math.max(0, values.indexOf(value));
  return (
    <div className={styles.field}>
      <span className={styles.fieldLabel}>{label}</span>
      <div className={styles.stepper}>
        <IconButton label={t("convert.previous", { name: label })} size={26} disabled={i === 0} onClick={() => onChange(values[i - 1])}>
          <ChevronLeft size={14} />
        </IconButton>
        <span className={`${styles.stepValue} tabular`} aria-live="polite">
          {format(values[i])}
        </span>
        <IconButton
          label={t("convert.next", { name: label })}
          size={26}
          disabled={i === values.length - 1}
          onClick={() => onChange(values[i + 1])}
        >
          <ChevronRight size={14} />
        </IconButton>
      </div>
    </div>
  );
}

function TimeField({ label, value, placeholder, onChange }: { label: string; value?: number; placeholder: string; onChange: (v?: number) => void }) {
  const island = useIsland();
  const [text, setText] = useState(value !== undefined ? formatClock(value) : "");
  const parsed = parseClock(text);
  const invalid = text.trim() !== "" && parsed === null;
  return (
    <label className={styles.field}>
      <span className={styles.fieldLabel}>{label}</span>
      <input
        className={styles.time}
        data-invalid={invalid}
        inputMode="numeric"
        value={text}
        placeholder={placeholder}
        aria-invalid={invalid}
        onClick={(e) => e.stopPropagation()}
        onFocus={() => island.setTyping(true)}
        onBlur={() => island.setTyping(false)}
        onChange={(e) => {
          setText(e.target.value);
          const v = parseClock(e.target.value);
          onChange(v ?? undefined);
        }}
      />
    </label>
  );
}

const orig = <T,>(v: T | undefined) => v as T | undefined;

function AdvancedPanel({ staged, target }: { staged: Staged; target: Target }) {
  const { t } = useTranslation();
  const a = staged.advanced;
  const set = (patch: Partial<Advanced>) => convertActions.setAdvanced(patch);
  const original = t("convert.original");
  const auto = t("convert.auto");
  const ok = usable(staged.files ?? []);
  const timed = ok.some((f) => f.media.kind !== "image");
  const hasAudio = ok.some((f) => f.media.hasAudio);
  const fields: React.ReactNode[] = [];

  if (target.output === "video") {
    fields.push(
      <Choice key="res" label={t("convert.resolution")} values={[undefined, 2160, 1440, 1080, 720, 480, 360]} value={orig(a.maxHeight)}
        format={(v) => (v ? `${v}p` : original)} onChange={(v) => set({ maxHeight: v })} />,
      <Choice key="fps" label={t("convert.frameRate")} values={[undefined, 60, 30, 24, 15]} value={orig(a.fps)}
        format={(v) => (v ? `${v} fps` : original)} onChange={(v) => set({ fps: v })} />,
    );
    if (hasAudio) {
      fields.push(
        <Choice key="mute" label={t("convert.sound")} values={[false, true]} value={!!a.mute}
          format={(v) => (v ? t("convert.off") : t("convert.on"))} onChange={(v) => set({ mute: v || undefined })} />,
      );
    }
  }
  if (target.output === "audio") {
    if (!target.lossless) {
      fields.push(
        <Choice key="br" label={t("convert.bitrate")} values={[undefined, 320, 256, 192, 160, 128, 96, 64]} value={orig(a.audioBitrate)}
          format={(v) => (v ? `${v} kbps` : auto)} onChange={(v) => set({ audioBitrate: v })} />,
      );
    }
    fields.push(
      <Choice key="sr" label={t("convert.sampleRate")} values={[undefined, 48000, 44100, 22050]} value={orig(a.sampleRate)}
        format={(v) => (v ? `${(v / 1000).toLocaleString()} kHz` : original)} onChange={(v) => set({ sampleRate: v })} />,
      <Choice key="ch" label={t("convert.channels")} values={[false, true]} value={!!a.mono}
        format={(v) => (v ? t("convert.mono") : original)} onChange={(v) => set({ mono: v || undefined })} />,
    );
  }
  if (target.output === "animation") {
    fields.push(
      <Choice key="w" label={t("convert.width")} values={[undefined, 1280, 960, 640, 480, 320]} value={orig(a.maxWidth)}
        format={(v) => (v ? `${v} px` : auto)} onChange={(v) => set({ maxWidth: v })} />,
      <Choice key="fps" label={t("convert.frameRate")} values={[undefined, 30, 25, 20, 15, 10]} value={orig(a.fps)}
        format={(v) => (v ? `${v} fps` : auto)} onChange={(v) => set({ fps: v })} />,
    );
  }
  if (target.output === "image") {
    fields.push(
      <Choice key="w" label={t("convert.width")} values={[undefined, 3840, 2560, 1920, 1280, 1024, 800, 512, 256]} value={orig(a.maxWidth)}
        format={(v) => (v ? `${v} px` : original)} onChange={(v) => set({ maxWidth: v })} />,
    );
  }
  if (timed && target.output !== "image") {
    fields.push(
      <TimeField key="start" label={t("convert.start")} value={a.start} placeholder="0:00" onChange={(v) => set({ start: v })} />,
      <TimeField key="end" label={t("convert.end")} value={a.end} placeholder={t("convert.toEnd")} onChange={(v) => set({ end: v })} />,
    );
  }
  return <div className={styles.advanced}>{fields}</div>;
}

const isSet = (a: Advanced) => Object.values(a).some((v) => v !== undefined && v !== false);

function Chooser({ staged }: { staged: Staged }) {
  const { t } = useTranslation();
  const files = staged.files;
  const targets = useMemo(() => commonTargets(files ?? []), [files]);
  const target = targets.find((x) => x.id === staged.target) ?? null;
  const ok = usable(files ?? []);
  const nothing = staged.files !== null && targets.length === 0;

  return (
    <div className={styles.chooser}>
      <Summary staged={staged} />
      <div className={styles.middle}>
        {staged.files === null ? (
          <p className={styles.note}>{t("convert.looking")}</p>
        ) : nothing ? (
          <p className={styles.note}>{ok.length ? t("convert.noCommon") : t("convert.unsupportedHint")}</p>
        ) : staged.showAdvanced && target ? (
          <AdvancedPanel staged={staged} target={target} />
        ) : (
          <TargetChips targets={targets} selected={staged.target} />
        )}
      </div>
      <div className={styles.footer}>
        {target?.lossless ? (
          <span className={styles.lossless}>{t("convert.lossless")}</span>
        ) : (
          <div className={styles.segmented} role="radiogroup" aria-label={t("convert.quality")}>
            {QUALITIES.map((q) => (
              <button
                key={q}
                type="button"
                role="radio"
                aria-checked={staged.quality === q}
                disabled={!target}
                onClick={(e) => {
                  e.stopPropagation();
                  convertActions.setQuality(q);
                }}
              >
                {t(`convert.qualities.${q}`)}
              </button>
            ))}
          </div>
        )}
        <span className={styles.advancedToggle} data-set={isSet(staged.advanced)}>
          <IconButton
            label={t("convert.advanced")}
            size={32}
            variant={staged.showAdvanced ? "filled" : "tinted"}
            tone={staged.showAdvanced ? ACCENT : GREY}
            pressed={staged.showAdvanced}
            disabled={!target}
            onClick={convertActions.toggleAdvanced}
          >
            <SlidersHorizontal size={15} />
          </IconButton>
        </span>
        <span className={styles.spacer} />
        <TextButton variant="filled" tone={ACCENT} disabled={!target || !ok.length} onClick={() => convertActions.start()}>
          {t("convert.convert")}
        </TextButton>
      </div>
    </div>
  );
}

function JobRow({ job, now }: { job: Job; now: number }) {
  const { t } = useTranslation();
  const active = job.status === "running" || job.status === "queued";
  const left = secondsLeft(job, now);
  let detail: string;
  switch (job.status) {
    case "queued":
      detail = t("convert.queued");
      break;
    case "running":
      detail = [
        job.progress === null ? t("convert.working") : `${Math.floor((job.progress ?? 0) * 100)}%`,
        left !== null ? t("convert.left", { time: formatClock(left) }) : null,
      ]
        .filter(Boolean)
        .join(" · ");
      break;
    case "done":
      detail = t("convert.done");
      break;
    case "failed":
      detail = job.error ?? t("convert.failed");
      break;
    case "cancelled":
      detail = t("convert.cancelled");
      break;
  }
  return (
    <li className={styles.job} data-status={job.status}>
      <div className={styles.meta}>
        <span className={styles.jobName}>
          {job.name} <span className={styles.arrow}>→ {ext(job.target)}</span>
        </span>
        {job.status === "running" ? (
          <div className={styles.jobProgress}>
            <div className={styles.bar} data-indeterminate={job.progress === null}>
              <div className={styles.fill} style={{ width: `${Math.max(2, (job.progress ?? 0) * 100)}%` }} />
            </div>
            <span className={`${styles.detail} tabular`}>{detail}</span>
          </div>
        ) : (
          <span className={`${styles.detail} tabular`} title={job.status === "failed" ? detail : undefined}>
            {detail}
          </span>
        )}
      </div>
      {active ? (
        <IconButton label={t("convert.cancel")} size={30} variant="tinted" tone={GREY} onClick={() => convertActions.cancel(job.id)}>
          <X size={14} />
        </IconButton>
      ) : job.status === "done" && job.output ? (
        <>
          <TextButton variant="tinted" tone={ACCENT} onClick={() => convertActions.open(job.output!)}>
            {t("convert.open")}
          </TextButton>
          <IconButton label={t("convert.showInFolder")} size={30} variant="tinted" tone={GREY} onClick={() => convertActions.reveal(job.output!)}>
            <FolderOpen size={14} />
          </IconButton>
        </>
      ) : null}
    </li>
  );
}

function Jobs({ jobs }: { jobs: Job[] }) {
  const { t } = useTranslation();
  const running = jobs.some((j) => j.status === "running");
  const now = useNow(running, 1000);
  const finished = jobs.some((j) => j.status !== "running" && j.status !== "queued");
  return (
    <div className={styles.jobs}>
      <div className={styles.jobsHead}>
        <span className={styles.groupLabel}>{t("convert.conversions")}</span>
        {finished && (
          <TextButton variant="plain" tone={GREY} onClick={convertActions.clear}>
            {t("convert.clear")}
          </TextButton>
        )}
      </div>
      <ul className={styles.jobList}>
        {[...jobs].reverse().map((job) => (
          <JobRow key={job.id} job={job} now={now} />
        ))}
      </ul>
      <p className={styles.hint}>{t("convert.dropMore")}</p>
    </div>
  );
}

function Empty({ tools, bytes }: { tools: Tools; bytes: number }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  return (
    <div className={styles.empty}>
      <span className={styles.bigTile}>
        <ArrowRightLeft size={26} />
      </span>
      <h2 className={styles.setupTitle}>{t("convert.dropHint")}</h2>
      <p className={styles.setupText}>{t("convert.emptyText")}</p>
      <p className={styles.toolsLine}>
        {tools.state === "ready" ? (
          <>
            {t("convert.toolsReady")}
            <TextButton
              variant="plain"
              tone={GREY}
              disabled={busy}
              onClick={() => {
                setBusy(true);
                convertActions.uninstall().finally(() => setBusy(false));
              }}
            >
              {t("convert.remove")}
            </TextButton>
          </>
        ) : (
          t("convert.toolsMissing", { size: formatBytes(bytes) })
        )}
      </p>
    </div>
  );
}

function Expanded() {
  const snap = useConvert();
  const staged = useStaged();
  if (staged) {
    return snap.tools.state === "ready" ? <Chooser staged={staged} /> : <Setup tools={snap.tools} bytes={snap.packageBytes} />;
  }
  if (snap.jobs.length) return <Jobs jobs={snap.jobs} />;
  return snap.tools.state === "downloading" ? (
    <Setup tools={snap.tools} bytes={snap.packageBytes} />
  ) : (
    <Empty tools={snap.tools} bytes={snap.packageBytes} />
  );
}

/** Working: above a working Claude session (50), below lyrics in the pill (55). */
const RUNNING: Activity = { priority: 52 };
/** Files waiting for a format. */
const STAGED: Activity = { priority: 45 };

function ConvertIcon({ size = 16, className }: IconProps) {
  return <ArrowRightLeft size={size} className={className} />;
}

export const convertApp: DuriApp = {
  id: "convert",
  nameKey: "apps.convert.name",
  descriptionKey: "apps.convert.description",
  Icon: ConvertIcon,
  accent: ACCENT,
  start: startConvert,
  useActivity() {
    const { jobs, tools } = useConvert();
    const fresh = useFresh();
    const staged = useStaged();
    const active = tools.state === "downloading" || jobs.some((j) => j.status === "running" || j.status === "queued");
    const freshKey = fresh.join(",");
    const hasStaged = !!staged;
    return useMemo<Activity | null>(
      () => (freshKey ? { priority: 70, alert: true, key: freshKey } : active ? RUNNING : hasStaged ? STAGED : null),
      [freshKey, active, hasStaged],
    );
  },
  Pill,
  pillWidth: 150,
  pillSide: 58,
  Minimal,
  Compact,
  compactSize: { w: 400, h: 72 },
  Expanded,
  Drop,
  dropSize: { w: 372, h: 84 },
  onDrop: (paths) => convertActions.stage(paths),
};
