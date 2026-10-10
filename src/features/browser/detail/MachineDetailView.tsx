import { useCallback } from "react";
import { useLoader } from "../../../shared/useLoader";
import { formatCount } from "../../catalog/catalogMessages";
import { DriverStatusBadge, Badge, StatusBadge } from "../badges";
import type { DetailTarget } from "../browserState";
import { getMachineDetail, type MachineDetail } from "../detailApi";
import { useInstallationScope } from "../../installations/installationsContext";
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
import { formatClock, formatSize, joinPresent, yesNo } from "./format";
import { RelatedList } from "./RelatedList";
import { MachineLaunchPanel } from "../../launcher/MachineLaunchPanel";

interface MachineDetailViewProps {
  name: string;
  tab: string;
  refreshKey: number;
  onTab: (tab: string) => void;
  onOpen: (target: DetailTarget) => void;
}

export function MachineDetailView({
  name,
  tab,
  refreshKey,
  onTab,
  onOpen,
}: MachineDetailViewProps) {
  const installationId = useInstallationScope();
  const load = useCallback(() => getMachineDetail(installationId, name), [installationId, name]);
  const { state, reload } = useLoader(load, refreshKey);

  if (state.status !== "ready" || state.value === null) {
    return <DetailStatus state={state} what="machine" onRetry={reload} />;
  }
  const machine = state.value;
  const tabs: DetailTab[] = [
    { id: "overview", label: "Overview" },
    { id: "roms", label: "ROMs", count: machine.roms.length },
    { id: "chds", label: "CHDs", count: machine.disks.length },
    {
      id: "devices",
      label: "Devices",
      count: machine.devices.length + machine.mediaDevices.length,
    },
    { id: "display", label: "Display", count: machine.displays.length },
    { id: "controls", label: "Controls", count: machine.controls.length },
    { id: "chipset", label: "Chipset", count: machine.chips.length },
    { id: "softwareLists", label: "Software lists", count: machine.softwareLists.length },
  ];
  const openMachine = (machineName: string) => {
    onOpen({ kind: "machine", name: machineName });
  };

  return (
    <DetailFrame
      title={machine.description}
      subtitle={
        <>
          <span className="code">{machine.name}</span>
          {[machine.manufacturer, machine.year].map((value) => value && ` · ${value}`)}
        </>
      }
      badges={<MachineBadges machine={machine} />}
      actions={<MachineLaunchPanel name={machine.name} isRunnable={machine.isRunnable} />}
      tabs={tabs}
      activeTab={tab}
      onTab={onTab}
    >
      {tab === "roms" ? (
        <RomsTab machine={machine} />
      ) : tab === "chds" ? (
        <DisksTab machine={machine} />
      ) : tab === "devices" ? (
        <DevicesTab machine={machine} onOpenMachine={openMachine} />
      ) : tab === "display" ? (
        <DisplayTab machine={machine} />
      ) : tab === "controls" ? (
        <ControlsTab machine={machine} />
      ) : tab === "chipset" ? (
        <ChipsetTab machine={machine} />
      ) : tab === "softwareLists" ? (
        <SoftwareListsTab machine={machine} onOpen={onOpen} />
      ) : (
        <OverviewTab
          machine={machine}
          refreshKey={refreshKey}
          onOpen={onOpen}
          onOpenMachine={openMachine}
        />
      )}
    </DetailFrame>
  );
}

function MachineBadges({ machine }: { machine: MachineDetail }) {
  return (
    <>
      <DriverStatusBadge status={machine.driver?.status ?? null} />
      <StatusBadge label="Emulation" status={machine.driver?.emulation ?? null} />
      {machine.parent && <Badge tone="neutral">Clone</Badge>}
      {machine.isBios && <Badge tone="neutral">BIOS</Badge>}
      {machine.isDevice && <Badge tone="neutral">Device</Badge>}
      {machine.isMechanical && <Badge tone="neutral">Mechanical</Badge>}
      {!machine.isRunnable && <Badge tone="neutral">Not runnable</Badge>}
    </>
  );
}

function OverviewTab({
  machine,
  refreshKey,
  onOpen,
  onOpenMachine,
}: {
  machine: MachineDetail;
  refreshKey: number;
  onOpen: (target: DetailTarget) => void;
  onOpenMachine: (name: string) => void;
}) {
  const driver = machine.driver;
  const flags = driver
    ? [
        driver.requiresArtwork && "Requires artwork",
        driver.unofficial && "Unofficial",
        driver.noSoundHardware && "No sound hardware",
        driver.incomplete && "Incomplete",
      ].filter((flag): flag is string => typeof flag === "string")
    : [];
  return (
    <>
      <Section title="General">
        <Fields
          label="General"
          fields={[
            ["Short name", <span className="code">{machine.name}</span>],
            ["Description", machine.description],
            ["Manufacturer", machine.manufacturer],
            ["Year", machine.year],
            [
              "Source file",
              machine.sourceFile && <span className="code">{machine.sourceFile}</span>,
            ],
            ["BIOS", yesNo(machine.isBios)],
            ["Device", yesNo(machine.isDevice)],
            ["Mechanical", yesNo(machine.isMechanical)],
            ["Runnable", yesNo(machine.isRunnable)],
          ]}
        />
      </Section>

      <Section title="Relations">
        <Fields
          label="Relations"
          fields={[
            [
              "Parent",
              machine.parent && <RecordLinkButton link={machine.parent} onOpen={onOpenMachine} />,
            ],
            [
              "ROMs from",
              machine.romParent && (
                <RecordLinkButton link={machine.romParent} onOpen={onOpenMachine} />
              ),
            ],
            ["Samples from", machine.sampleOf && <span className="code">{machine.sampleOf}</span>],
            ["Clones", formatCount(machine.cloneCount)],
          ]}
        />
        {machine.cloneCount > 0 && (
          <RelatedList
            kind="machineClones"
            name={machine.name}
            label="Clones"
            refreshKey={refreshKey}
            onOpen={onOpen}
          />
        )}
      </Section>

      <Section title="Emulation">
        {driver ? (
          <Fields
            label="Emulation"
            fields={[
              ["Driver status", driver.status],
              ["Emulation", driver.emulation],
              ["Cocktail", driver.cocktail],
              ["Save states", driver.saveState],
              ["Notes", flags.join(", ")],
            ]}
          />
        ) : (
          <Empty>No driver information is declared.</Empty>
        )}
        {machine.features.length > 0 && (
          <DataTable
            label="Emulation features"
            columns={["Feature", "Status", "Overall"]}
            rows={machine.features.map((feature, index) => ({
              key: String(index),
              cells: [feature.featureType, feature.status, feature.overall],
            }))}
          />
        )}
      </Section>

      {machine.samples.length > 0 && (
        <Section title="Samples">
          <p className="code wrap">{machine.samples.join(", ")}</p>
        </Section>
      )}
    </>
  );
}

function RomsTab({ machine }: { machine: MachineDetail }) {
  return (
    <>
      <p className="message message--muted">
        ROMs declared by MAME for this machine. This does not tell whether the files are on disk.
      </p>
      {machine.biosSets.length > 0 && (
        <Section title="BIOS sets">
          <DataTable
            label="BIOS sets"
            columns={["Name", "Description", "Default"]}
            rows={machine.biosSets.map((bios) => ({
              key: bios.name,
              cells: [
                <span className="code">{bios.name}</span>,
                bios.description,
                yesNo(bios.isDefault),
              ],
            }))}
          />
        </Section>
      )}
      <Section title="ROMs">
        {machine.roms.length === 0 ? (
          <Empty>No ROMs are declared.</Empty>
        ) : (
          <DataTable
            label="ROMs"
            columns={["Name", "Size", "CRC", "SHA-1", "Region", "Merge", "BIOS", "Status"]}
            rows={machine.roms.map((rom, index) => ({
              key: String(index),
              cells: [
                <span className="code">{rom.name}</span>,
                formatSize(rom.size),
                rom.crc && <span className="code">{rom.crc}</span>,
                rom.sha1 && <span className="code hash">{rom.sha1}</span>,
                rom.region,
                rom.mergeName,
                rom.bios,
                rom.isOptional ? `${rom.status} (optional)` : rom.status,
              ],
            }))}
          />
        )}
      </Section>
    </>
  );
}

function DisksTab({ machine }: { machine: MachineDetail }) {
  return (
    <>
      <p className="message message--muted">
        CHDs declared by MAME for this machine. This does not tell whether the files are on disk.
      </p>
      {machine.disks.length === 0 ? (
        <Empty>No CHDs are declared.</Empty>
      ) : (
        <DataTable
          label="CHDs"
          columns={["Name", "SHA-1", "Region", "Index", "Merge", "Writable", "Status"]}
          rows={machine.disks.map((disk, index) => ({
            key: String(index),
            cells: [
              <span className="code">{disk.name}</span>,
              disk.sha1 && <span className="code hash">{disk.sha1}</span>,
              disk.region,
              disk.index,
              disk.mergeName,
              yesNo(disk.isWritable),
              disk.isOptional ? `${disk.status} (optional)` : disk.status,
            ],
          }))}
        />
      )}
    </>
  );
}

function DevicesTab({
  machine,
  onOpenMachine,
}: {
  machine: MachineDetail;
  onOpenMachine: (name: string) => void;
}) {
  return (
    <>
      <Section title="Devices">
        {machine.devices.length === 0 ? (
          <Empty>No devices are declared.</Empty>
        ) : (
          <DataTable
            label="Devices"
            columns={["Device", "Instances"]}
            rows={machine.devices.map((device) => ({
              key: device.device.name,
              cells: [
                <RecordLinkButton link={device.device} onOpen={onOpenMachine} />,
                device.instanceCount,
              ],
            }))}
          />
        )}
      </Section>
      {machine.mediaDevices.length > 0 && (
        <Section title="Media devices">
          <DataTable
            label="Media devices"
            columns={["Type", "Instance", "Tag", "Interface", "Extensions", "Mandatory"]}
            rows={machine.mediaDevices.map((device, index) => ({
              key: String(index),
              cells: [
                device.deviceType,
                joinPresent([device.instanceName, device.instanceBriefName], " / "),
                device.tag,
                device.interface,
                device.extensions.join(", "),
                device.mandatory === "1" ? "Yes" : null,
              ],
            }))}
          />
        </Section>
      )}
      {machine.ramOptions.length > 0 && (
        <Section title="RAM options">
          <DataTable
            label="RAM options"
            columns={["Name", "Size", "Default"]}
            rows={machine.ramOptions.map((option, index) => ({
              key: String(index),
              cells: [option.name, formatSize(option.size), yesNo(option.isDefault)],
            }))}
          />
        </Section>
      )}
    </>
  );
}

function DisplayTab({ machine }: { machine: MachineDetail }) {
  return (
    <>
      <Section title="Displays">
        {machine.displays.length === 0 ? (
          <Empty>No displays are declared.</Empty>
        ) : (
          <DataTable
            label="Displays"
            columns={["Tag", "Type", "Resolution", "Refresh", "Rotation", "Flipped"]}
            rows={machine.displays.map((display, index) => ({
              key: String(index),
              cells: [
                display.tag,
                display.displayType,
                display.width !== null && display.height !== null
                  ? `${String(display.width)} × ${String(display.height)}`
                  : null,
                display.refresh !== null ? `${display.refresh.toFixed(6)} Hz` : null,
                display.rotate !== null ? `${String(display.rotate)}°` : null,
                yesNo(display.flipX),
              ],
            }))}
          />
        )}
      </Section>
      <Section title="Sound">
        <Fields label="Sound" fields={[["Channels", machine.soundChannels]]} />
        {machine.soundChannels === null && <Empty>No sound information is declared.</Empty>}
      </Section>
    </>
  );
}

function ControlsTab({ machine }: { machine: MachineDetail }) {
  const input = machine.input;
  return (
    <>
      <Section title="Input">
        {input ? (
          <Fields
            label="Input"
            fields={[
              ["Players", input.players],
              ["Coin slots", input.coins],
              ["Service", yesNo(input.service)],
              ["Tilt", yesNo(input.tilt)],
            ]}
          />
        ) : (
          <Empty>No input information is declared.</Empty>
        )}
      </Section>
      {machine.controls.length > 0 && (
        <Section title="Controls">
          <DataTable
            label="Controls"
            columns={["Type", "Player", "Buttons", "Required buttons", "Ways"]}
            rows={machine.controls.map((control, index) => ({
              key: String(index),
              cells: [
                control.controlType,
                control.player,
                control.buttons,
                control.requiredButtons,
                joinPresent([control.ways, control.ways2, control.ways3], " / "),
              ],
            }))}
          />
        </Section>
      )}
    </>
  );
}

function ChipsetTab({ machine }: { machine: MachineDetail }) {
  if (machine.chips.length === 0) return <Empty>No chips are declared.</Empty>;
  return (
    <DataTable
      label="Chips"
      columns={["Type", "Name", "Tag", "Clock"]}
      rows={machine.chips.map((chip, index) => ({
        key: String(index),
        cells: [chip.chipType, chip.name, chip.tag, formatClock(chip.clock)],
      }))}
    />
  );
}

function SoftwareListsTab({
  machine,
  onOpen,
}: {
  machine: MachineDetail;
  onOpen: (target: DetailTarget) => void;
}) {
  if (machine.softwareLists.length === 0) return <Empty>No software lists are declared.</Empty>;
  return (
    <>
      <p className="message message--muted">
        Software lists declared by MAME for this machine. A declared list does not guarantee that
        every software in it works with this machine.
      </p>
      <DataTable
        label="Software lists"
        columns={["List", "Status", "Filter", "Tag"]}
        rows={machine.softwareLists.map((list, index) => ({
          key: `${list.name}-${String(index)}`,
          cells: [
            <span>
              <button
                type="button"
                className="link-button"
                onClick={() => {
                  onOpen({ kind: "softwareList", name: list.name });
                }}
              >
                {list.description ?? list.name}
              </button>{" "}
              <span className="code">({list.name})</span>
              {!list.available && <span className="message--muted"> · not imported</span>}
            </span>,
            list.status,
            list.filter,
            list.tag,
          ],
        }))}
      />
    </>
  );
}
