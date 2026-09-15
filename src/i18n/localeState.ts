import { UiLocale } from "../types/settings";

let testFallbackLocale: UiLocale | null = null;

export const getTestFallbackLocale = (): UiLocale | null => testFallbackLocale;

export const setTestFallbackLocale = (loc: UiLocale | null): void => {
  testFallbackLocale = loc;
};
