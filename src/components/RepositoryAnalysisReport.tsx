import React, { useState } from "react";
import {
  AnalysisEnvelopeV1,
  AnalysisSection,
  ClaimBasis,
  EvidenceRecord,
  SectionId,
  SectionStatus,
} from "../types/library";
import { openGitHubUrl } from "../services/tauri";
import { useI18n } from "../i18n/I18nContext";

export interface RepositoryAnalysisReportProps {
  envelope: AnalysisEnvelopeV1;
  className?: string;
}

const SECTION_KEYS: SectionId[] = [
  "overview",
  "features",
  "use_cases",
  "tech_stack",
  "setup",
  "target_users",
  "maturity",
  "limitations",
];

export const RepositoryAnalysisReport: React.FC<RepositoryAnalysisReportProps> = ({
  envelope,
  className = "",
}) => {
  const { t } = useI18n();
  const [sourcesExpanded, setSourcesExpanded] = useState(false);

  const { content, generator, source, coverage, evidence } = envelope;

  const getStatusBadge = (status: SectionStatus) => {
    switch (status) {
      case "supported":
        return (
          <span
            data-testid="status-badge-supported"
            className="inline-flex items-center px-2 py-0.5 rounded text-[11px] font-medium bg-emerald-950/70 text-emerald-300 border border-emerald-700/60"
          >
            {t("analysis.status.supported")}
          </span>
        );
      case "partial":
        return (
          <span
            data-testid="status-badge-partial"
            className="inline-flex items-center px-2 py-0.5 rounded text-[11px] font-medium bg-amber-950/70 text-amber-300 border border-amber-700/60"
          >
            {t("analysis.status.partial")}
          </span>
        );
      case "insufficient":
      default:
        return (
          <span
            data-testid="status-badge-insufficient"
            className="inline-flex items-center px-2 py-0.5 rounded text-[11px] font-medium bg-slate-800 text-slate-400 border border-slate-700"
          >
            {t("analysis.status.insufficient")}
          </span>
        );
    }
  };

  const getBasisBadge = (basis: ClaimBasis) => {
    switch (basis) {
      case "documented":
        return (
          <span
            data-testid="basis-badge-documented"
            className="inline-flex items-center px-1.5 py-0.2 rounded text-[10px] font-medium bg-blue-950/60 text-blue-300 border border-blue-800/60"
          >
            {t("analysis.basis.documented")}
          </span>
        );
      case "code_observed":
        return (
          <span
            data-testid="basis-badge-code_observed"
            className="inline-flex items-center px-1.5 py-0.2 rounded text-[10px] font-medium bg-purple-950/60 text-purple-300 border border-purple-800/60"
          >
            {t("analysis.basis.code_observed")}
          </span>
        );
      case "inferred":
        return (
          <span
            data-testid="basis-badge-inferred"
            className="inline-flex items-center px-1.5 py-0.2 rounded text-[10px] font-medium bg-amber-950/50 text-amber-400 border border-amber-800/50"
          >
            {t("analysis.basis.inferred")}
          </span>
        );
      case "uncertain":
      default:
        return (
          <span
            data-testid="basis-badge-uncertain"
            className="inline-flex items-center px-1.5 py-0.2 rounded text-[10px] font-medium bg-rose-950/60 text-rose-300 border border-rose-800/60"
          >
            {t("analysis.basis.uncertain")}
          </span>
        );
    }
  };

  const handleOpenSource = async (record: EvidenceRecord) => {
    if (!source.canonical_url || !record.path) return;
    try {
      const commitSha = record.commit_sha || source.commit_sha || source.default_branch || "main";
      let url = `${source.canonical_url.replace(/\/+$/, "")}/blob/${commitSha}/${record.path.replace(/^\/+/, "")}`;
      if (record.included_ranges && record.included_ranges.length > 0) {
        const first = record.included_ranges[0];
        url += `#L${first.start_line}-L${first.end_line}`;
      }
      // Safety check: ensure URL starts with canonical GitHub URL
      if (url.startsWith("https://github.com/")) {
        await openGitHubUrl(url);
      }
    } catch (err) {
      console.error("Failed to open source URL safely:", err);
    }
  };

  const formattedGeneratedAt = envelope.generated_at
    ? new Date(envelope.generated_at).toLocaleString()
    : t("common.unknown");

  return (
    <div
      data-testid="repository-analysis-report"
      className={`space-y-6 text-slate-200 ${className}`}
    >
      {/* Header metadata & disclaimer */}
      <div
        data-testid="analysis-metadata-card"
        className="rounded-lg border border-slate-700/80 bg-slate-900/90 p-4 space-y-3"
      >
        <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-slate-400 border-b border-slate-800 pb-2.5">
          <div className="flex items-center gap-3 flex-wrap">
            <span data-testid="report-meta-generated-at">
              <strong className="text-slate-300">{t("analysis.generatedAt")}: </strong>
              {formattedGeneratedAt}
            </span>
            <span data-testid="report-meta-provider">
              <strong className="text-slate-300">{t("analysis.provider")}: </strong>
              <span className="font-mono text-cyan-300">{generator.provider}</span>
            </span>
            <span data-testid="report-meta-model-requested">
              <strong className="text-slate-300">{t("analysis.modelRequested")}: </strong>
              <span className="font-mono text-indigo-300">{generator.model_requested}</span>
            </span>
            {generator.model_reported && (
              <span data-testid="report-meta-model-reported">
                <strong className="text-slate-300">{t("analysis.modelReported")}: </strong>
                <span className="font-mono text-emerald-300">{generator.model_reported}</span>
              </span>
            )}
            <span data-testid="report-meta-locale">
              <strong className="text-slate-300">{t("analysis.targetLocale")}: </strong>
              <span className="uppercase text-slate-300">{envelope.target_locale}</span>
            </span>
          </div>

          <div className="flex items-center gap-2 flex-wrap">
            <span
              data-testid="report-meta-coverage"
              className={`px-2 py-0.5 rounded text-[11px] font-medium border ${
                coverage.status === "bounded"
                  ? "bg-indigo-950 text-indigo-300 border-indigo-700/60"
                  : "bg-amber-950 text-amber-300 border-amber-700/60"
              }`}
            >
              {coverage.status === "bounded"
                ? t("analysis.coverageBounded")
                : t("analysis.coveragePartial")}
              {" "}({coverage.sent_file_count}/{coverage.selected_file_count} {t("analysis.filesCount")})
            </span>
          </div>
        </div>

        {/* Disclaimer */}
        <p
          data-testid="analysis-disclaimer"
          className="text-xs text-slate-400 italic bg-slate-950/50 p-2.5 rounded border border-slate-800/80"
        >
          {t("analysis.disclaimer")}
        </p>

        {/* AC111 External Send Disclosure */}
        <div
          data-testid="analysis-disclosure"
          className="text-xs text-slate-400 bg-slate-950/40 p-2.5 rounded border border-slate-800/80 space-y-1"
        >
          <p data-testid="analysis-evidence-sent-disclosure">
            {t("analysis.disclosure.evidenceSent", { provider: generator.provider })}
          </p>
          {generator.provider?.toLowerCase() === "ollama" ? (
            <p data-testid="analysis-ollama-privacy-note" className="text-[11px] text-slate-500">
              {t("analysis.disclosure.ollamaNote")}
            </p>
          ) : (
            <p data-testid="analysis-cloud-privacy-note" className="text-[11px] text-slate-500">
              {t("analysis.disclosure.cloudNote")}
            </p>
          )}
        </div>
      </div>

      {/* 8 Sections Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {SECTION_KEYS.map((key) => {
          const section: AnalysisSection = content[key] || {
            status: "insufficient",
            items: [],
            missing_information: [],
          };
          const title = t(`analysis.sections.${key}`);

          return (
            <div
              key={key}
              data-testid={`analysis-section-${key}`}
              className="rounded-lg border border-slate-700/80 bg-slate-800/70 p-4 space-y-3 flex flex-col justify-between"
            >
              <div>
                <div className="flex items-center justify-between gap-2 border-b border-slate-700/60 pb-2">
                  <h4 className="text-sm font-semibold text-slate-100">{title}</h4>
                  {getStatusBadge(section.status)}
                </div>

                {/* Items */}
                {section.items && section.items.length > 0 ? (
                  <ul className="mt-2.5 space-y-2 text-xs">
                    {section.items.map((item, idx) => (
                      <li
                        key={idx}
                        data-testid={`analysis-item-${key}-${idx}`}
                        className="flex flex-col gap-1 rounded bg-slate-900/60 p-2 border border-slate-800/60"
                      >
                        <div className="flex items-start justify-between gap-2">
                          <span className="text-slate-200 leading-relaxed break-words flex-1">
                            {item.text}
                          </span>
                          {getBasisBadge(item.basis)}
                        </div>
                        {item.evidence_ids && item.evidence_ids.length > 0 && (
                          <div className="flex items-center gap-1 flex-wrap text-[10px] text-slate-500 pt-0.5">
                            <span>{t("analysis.evidence")}:</span>
                            {item.evidence_ids.map((eid) => (
                              <span
                                key={eid}
                                className="font-mono bg-slate-800 px-1 py-0.2 rounded text-slate-400"
                              >
                                #{eid}
                              </span>
                            ))}
                          </div>
                        )}
                      </li>
                    ))}
                  </ul>
                ) : (
                  <div className="mt-2.5 text-xs text-slate-500 italic p-2">
                    {t("analysis.status.insufficient")}
                  </div>
                )}
              </div>

              {/* Missing Information */}
              {section.missing_information && section.missing_information.length > 0 && (
                <div
                  data-testid={`analysis-missing-${key}`}
                  className="rounded bg-amber-950/20 border border-amber-900/40 p-2 text-xs text-amber-300/80 space-y-1"
                >
                  <span className="font-semibold text-[11px] block">
                    {t("analysis.missingInfoTitle")}:
                  </span>
                  <ul className="list-disc list-inside space-y-0.5">
                    {section.missing_information.map((m, mIdx) => (
                      <li key={mIdx} className="break-words">
                        {m}
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </div>
          );
        })}
      </div>

      {/* Sources & Evidence Collapsible Drawer */}
      {evidence && evidence.length > 0 && (
        <div className="rounded-lg border border-slate-700/80 bg-slate-900/80 p-4 space-y-3">
          <div className="flex items-center justify-between">
            <h4 className="text-xs font-semibold text-slate-300 uppercase tracking-wider">
              {t("analysis.sourcesTitle")} ({evidence.length} {t("analysis.evidenceCount")})
            </h4>
            <button
              type="button"
              onClick={() => setSourcesExpanded(!sourcesExpanded)}
              aria-expanded={sourcesExpanded}
              data-testid="toggle-evidence-btn"
              className="text-xs text-indigo-400 hover:text-indigo-300 font-medium"
            >
              {sourcesExpanded ? t("analysis.collapseSources") : t("analysis.expandSources")}
            </button>
          </div>

          {sourcesExpanded && (
            <div data-testid="analysis-evidence-drawer" className="space-y-2 pt-2 border-t border-slate-800">
              {evidence.map((rec) => {
                const canOpen = Boolean(rec.path && source.canonical_url);
                return (
                  <div
                    key={rec.id}
                    data-testid={`evidence-record-${rec.id}`}
                    className="flex flex-wrap items-center justify-between gap-2 p-2 rounded bg-slate-800/80 text-xs border border-slate-700/60"
                  >
                    <div className="flex items-center gap-2 flex-wrap min-w-0">
                      <span className="font-mono text-[11px] font-bold text-indigo-300">
                        #{rec.id}
                      </span>
                      <span className="px-1.5 py-0.2 rounded text-[10px] font-medium bg-slate-700 text-slate-300 uppercase">
                        {rec.kind}
                      </span>
                      <span className="font-mono text-slate-200 truncate max-w-xs md:max-w-md">
                        {rec.path || source.canonical_url}
                      </span>
                      {rec.included_ranges && rec.included_ranges.length > 0 && (
                        <span className="text-[10px] text-slate-400 font-mono">
                          L{rec.included_ranges[0].start_line}-L{rec.included_ranges[0].end_line}
                        </span>
                      )}
                      {rec.truncated && (
                        <span className="text-[10px] text-amber-400 bg-amber-950/60 px-1 rounded">
                          truncated
                        </span>
                      )}
                    </div>

                    {canOpen ? (
                      <button
                        type="button"
                        onClick={() => handleOpenSource(rec)}
                        data-testid={`open-source-${rec.id}`}
                        className="text-xs text-indigo-400 hover:text-indigo-300 font-medium hover:underline shrink-0"
                      >
                        {t("analysis.openInGitHub")} ↗
                      </button>
                    ) : (
                      <span className="text-slate-500 text-[10px] italic">
                        {t("analysis.source")}
                      </span>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
