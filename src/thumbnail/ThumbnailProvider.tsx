import { createContext, useContext, type ReactNode } from "react";
import { ThumbnailClient } from "./client";
import { browserFixtureThumbnailAdapter } from "./fixtureAdapter";

const browserFixtureThumbnailClient = new ThumbnailClient(browserFixtureThumbnailAdapter);
const ThumbnailClientContext = createContext(browserFixtureThumbnailClient);

export function ThumbnailProvider({ client, children }: { client: ThumbnailClient; children: ReactNode }) {
  return <ThumbnailClientContext.Provider value={client}>{children}</ThumbnailClientContext.Provider>;
}

export function useThumbnailClient(override?: ThumbnailClient): ThumbnailClient {
  const contextClient = useContext(ThumbnailClientContext);
  return override ?? contextClient;
}
