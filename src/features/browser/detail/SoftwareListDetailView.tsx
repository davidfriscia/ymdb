import { useCallback } from "react";
import { useLoader } from "../../../shared/useLoader";
import { formatCount } from "../../catalog/catalogMessages";
import type { DetailTarget } from "../browserState";
import { getSoftwareListDetail } from "../detailApi";
import { useInstallationScope } from "../../installations/installationsContext";
import { DetailFrame, DetailStatus, Fields, Section, type DetailTab } from "./DetailParts";
import { RelatedList } from "./RelatedList";

interface SoftwareListDetailViewProps {
  name: string;
  tab: string;
  refreshKey: number;
  /** Whether software lists are imported in the catalog. */
  softwareImported: boolean;
  onTab: (tab: string) => void;
  onOpen: (target: DetailTarget) => void;
}

export function SoftwareListDetailView({
  name,
  tab,
  refreshKey,
  softwareImported,
  onTab,
  onOpen,
}: SoftwareListDetailViewProps) {
  const installationId = useInstallationScope();
  const load = useCallback(
    () => getSoftwareListDetail(installationId, name),
    [installationId, name],
  );
  const { state, reload } = useLoader(load, refreshKey);

  if (state.status !== "ready" || state.value === null) {
    return <DetailStatus state={state} what="software list" onRetry={reload} />;
  }
  const list = state.value;
  const machineCount = list.originalMachineCount + list.compatibleMachineCount;
  const tabs: DetailTab[] = [
    { id: "overview", label: "Overview" },
    ...(list.available ? [{ id: "software", label: "Software", count: list.softwareCount }] : []),
    { id: "machines", label: "Machines", count: machineCount },
  ];

  return (
    <DetailFrame
      title={list.description ?? list.name}
      subtitle={
        <>
          Software list <span className="code">{list.name}</span>
        </>
      }
      tabs={tabs}
      activeTab={tab}
      onTab={onTab}
    >
      {tab === "software" && list.available ? (
        <RelatedList
          kind="listSoftware"
          list={list.name}
          label="Software"
          refreshKey={refreshKey}
          onOpen={onOpen}
        />
      ) : tab === "machines" ? (
        <>
          <p className="message message--muted">
            Machines declaring this list. A declared list does not guarantee that every software in
            it works with every machine.
          </p>
          <RelatedList
            kind="listMachines"
            list={list.name}
            label="Machines"
            refreshKey={refreshKey}
            onOpen={onOpen}
          />
        </>
      ) : (
        <>
          {!list.available && (
            <p className="message message--warning" role="status">
              {softwareImported
                ? "This list is referenced by machines but is not provided by this MAME version."
                : "The software lists have not been imported yet. Import them from the MAME page."}
            </p>
          )}
          <Section title="General">
            <Fields
              label="General"
              fields={[
                ["Name", <span className="code">{list.name}</span>],
                ["Description", list.description],
                ["Software", list.available ? formatCount(list.softwareCount) : null],
                ["Original machines", formatCount(list.originalMachineCount)],
                ["Compatible machines", formatCount(list.compatibleMachineCount)],
                ["Notes", list.notes && <span className="wrap">{list.notes}</span>],
              ]}
            />
          </Section>
        </>
      )}
    </DetailFrame>
  );
}
