import type { ReactNode } from "react";
import "./Layout.css";

interface LayoutProps {
  titleBar?: ReactNode;
  sidebar?: ReactNode;
  fileExplorer?: ReactNode;
  metadataPanel?: ReactNode;
  waveformBar?: ReactNode;
  playbackControls?: ReactNode;
  spectrum?: ReactNode;
  vuMeter?: ReactNode;
}

export default function Layout({
  titleBar,
  sidebar,
  fileExplorer,
  metadataPanel,
  waveformBar,
  playbackControls,
  spectrum,
  vuMeter,
}: LayoutProps) {
  return (
    <div className="layout">
      <div className="layout__titlebar">
        {titleBar ?? <div className="layout__placeholder">TitleBar</div>}
      </div>
      <div className="layout__main">
        <div className="layout__sidebar">
          {sidebar ?? <div className="layout__placeholder">Sidebar</div>}
        </div>
        <div className="layout__explorer">
          {fileExplorer ?? <div className="layout__placeholder">FileExplorer</div>}
        </div>
        <div className="layout__metadata">
          {metadataPanel ?? <div className="layout__placeholder">MetadataPanel</div>}
        </div>
      </div>
      <div className="layout__waveform">
        {waveformBar ?? <div className="layout__placeholder">WaveformBar</div>}
      </div>
      <div className="layout__playback">
        <div className="layout__playback-controls">
          {playbackControls ?? <div className="layout__placeholder">PlaybackControls</div>}
        </div>
        {spectrum && <div className="layout__spectrum">{spectrum}</div>}
        {vuMeter && <div className="layout__vu">{vuMeter}</div>}
      </div>
    </div>
  );
}
