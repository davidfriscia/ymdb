import { useInstallationScope } from "../../installations/installationsContext";
import { useCallback } from "react";
import { useLoader } from "../../../shared/useLoader";
import { formatCount } from "../../catalog/catalogMessages";
import { Badge, SupportBadge } from "../badges";
import type { DetailTarget } from "../browserState";
import {
  getSoftwareDetail,
  type NameValue,
  type PartDetail,
  type SoftwareDetail,
} from "../detailApi";
import {
  DataTable,
  DetailFrame,
  DetailStatus,
  Empty,
  Fields,
  RecordLinkButton,
  Section,
  type DetailTab,
} from "./DetailParts";
import { formatSize } from "./format";
import { RelatedList } from "./RelatedList";
import { SoftwareLaunchPanel } from "../../launcher/SoftwareLaunchPanel";

interface SoftwareDetailViewProps {
  list: string;
  name: string;
  tab: string;
  refreshKey: number;
  onTab: (tab: string) => void;
  onOpen: (target: DetailTarget) => void;
}

export function SoftwareDetailView({
  list,
  name,
  tab,
  refreshKey,
  onTab,
  onOpen,
}: SoftwareDetailViewProps) {
  const installationId = useInstallationScope();
  const load = useCallback(
    () => getSoftwareDetail(installationId, list, name),
    [installationId, list, name],
  );
  const { state, reload } = useLoader(load, refreshKey);

  if (state.status !== "ready" || state.value === null) {
    return <DetailStatus state={state} what="software" onRetry={reload} />;
  }
  const software = state.value;
  const tabs: DetailTab[] = [
    { id: "overview", label: "Overview" },
    { id: "parts", label: "Parts", count: software.parts.length },
    { id: "machines", label: "Machines", count: software.machineCount },
  ];

  return (
    <DetailFrame
      title={software.description}
      subtitle={
        <>
          <span className="code">
            {software.listName}/{software.name}
          </span>
          {[software.publisher, software.year].map((value) => value && ` · ${value}`)}
        </>
      }
      badges={
        <>
          <SupportBadge supported={software.supported} />
          {software.parent && <Badge tone="neutral">Clone</Badge>}
        </>
      }
      media={{ kind: "software", list: software.listName, name: software.name }}
      actions={
        <SoftwareLaunchPanel
          list={software.listName}
          name={software.name}
          refreshKey={refreshKey}
        />
      }
      tabs={tabs}
      activeTab={tab}
      onTab={onTab}
    >
      {tab === "parts" ? (
        <PartsTab parts={software.parts} />
      ) : tab === "machines" ? (
        <>
          <p className="message message--muted">
            Machines declaring the software list {software.listName}. A declared list does not
            guarantee that this software works with every machine.
          </p>
          <RelatedList
            kind="listMachines"
            list={software.listName}
            label="Machines"
            refreshKey={refreshKey}
            onOpen={onOpen}
          />
        </>
      ) : (
        <OverviewTab software={software} refreshKey={refreshKey} onOpen={onOpen} />
      )}
    </DetailFrame>
  );
}

function NameValues({ label, values }: { label: string; values: NameValue[] }) {
  if (values.length === 0) return null;
  return (
    <DataTable
      label={label}
      columns={["Name", "Value"]}
      rows={values.map((entry, index) => ({
        key: String(index),
        cells: [entry.name, entry.value],
      }))}
    />
  );
}

function OverviewTab({
  software,
  refreshKey,
  onOpen,
}: {
  software: SoftwareDetail;
  refreshKey: number;
  onOpen: (target: DetailTarget) => void;
}) {
  const openSibling = (name: string) => {
    onOpen({ kind: "software", list: software.listName, name });
  };
  return (
    <>
      <Section title="General">
        <Fields
          label="General"
          fields={[
            ["Short name", <span className="code">{software.name}</span>],
            ["Description", software.description],
            [
              "Software list",
              <button
                type="button"
                className="link-button"
                onClick={() => {
                  onOpen({ kind: "softwareList", name: software.listName });
                }}
              >
                {software.listDescription ?? software.listName}{" "}
                <span className="code">({software.listName})</span>
              </button>,
            ],
            ["Publisher", software.publisher],
            ["Year", software.year],
            ["Support", <SupportBadge supported={software.supported} />],
            ["Notes", software.notes && <span className="wrap">{software.notes}</span>],
          ]}
        />
      </Section>

      <Section title="Relations">
        <Fields
          label="Relations"
          fields={[
            [
              "Parent",
              software.parent && <RecordLinkButton link={software.parent} onOpen={openSibling} />,
            ],
            ["Clones", formatCount(software.cloneCount)],
          ]}
        />
        {software.cloneCount > 0 && (
          <RelatedList
            kind="softwareClones"
            list={software.listName}
            name={software.name}
            label="Clones"
            refreshKey={refreshKey}
            onOpen={onOpen}
          />
        )}
      </Section>

      {(software.info.length > 0 || software.sharedFeatures.length > 0) && (
        <Section title="Information">
          <NameValues label="Information" values={software.info} />
          <NameValues label="Shared features" values={software.sharedFeatures} />
        </Section>
      )}
    </>
  );
}

function PartsTab({ parts }: { parts: PartDetail[] }) {
  if (parts.length === 0) return <Empty>No parts are declared.</Empty>;
  return (
    <>
      <p className="message message--muted">
        Media declared by MAME for this software. This does not tell whether the files are on disk.
      </p>
      {parts.map((part) => (
        <Section key={part.name} title={`Part ${part.name} · ${part.interface}`}>
          <NameValues label={`Features of ${part.name}`} values={part.features} />
          {part.dataAreas.map((area) => (
            <div key={area.name} className="detail__subsection">
              <h4 className="detail__subsection-title">
                Data area {area.name} · {formatSize(area.size)} · {area.databits}-bit {area.endian}
              </h4>
              {area.roms.length === 0 ? (
                <Empty>No ROMs.</Empty>
              ) : (
                <DataTable
                  label={`ROMs of ${area.name}`}
                  columns={["Name", "Size", "CRC", "SHA-1", "Offset", "Load", "Status"]}
                  rows={area.roms.map((rom, index) => ({
                    key: String(index),
                    cells: [
                      rom.name && <span className="code">{rom.name}</span>,
                      formatSize(rom.size ?? rom.length),
                      rom.crc && <span className="code">{rom.crc}</span>,
                      rom.sha1 && <span className="code hash">{rom.sha1}</span>,
                      rom.offset && <span className="code">{rom.offset}</span>,
                      rom.loadFlag && [rom.loadFlag, rom.value].filter(Boolean).join(" "),
                      rom.status,
                    ],
                  }))}
                />
              )}
            </div>
          ))}
          {part.diskAreas.map((area) => (
            <div key={area.name} className="detail__subsection">
              <h4 className="detail__subsection-title">Disk area {area.name}</h4>
              <DataTable
                label={`Disks of ${area.name}`}
                columns={["Name", "SHA-1", "Writeable", "Status"]}
                rows={area.disks.map((disk, index) => ({
                  key: String(index),
                  cells: [
                    <span className="code">{disk.name}</span>,
                    disk.sha1 && <span className="code hash">{disk.sha1}</span>,
                    disk.isWriteable ? "Yes" : "No",
                    disk.status,
                  ],
                }))}
              />
            </div>
          ))}
        </Section>
      ))}
    </>
  );
}
