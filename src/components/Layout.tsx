import type { ReactNode } from "react";
import "./Layout.css";

interface LayoutProps {
  titleBar?: ReactNode;
  sidebar?: ReactNode;
  fileExplorer?: ReactNode;
  metadataPanel?: ReactNode;
  bottomBar?: ReactNode;
}

export default function Layout({
  titleBar,
  sidebar,
  fileExplorer,
  metadataPanel,
  bottomBar,
}: LayoutProps) {
  return (
    <div className="layout">
      <div className="layout__titlebar">{titleBar}</div>
      <div className="layout__main">
        <div className="layout__sidebar">{sidebar}</div>
        <div className="layout__explorer">{fileExplorer}</div>
        <div className="layout__metadata">{metadataPanel}</div>
      </div>
      <div className="layout__bottom">{bottomBar}</div>
    </div>
  );
}
