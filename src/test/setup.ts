import "@testing-library/jest-dom";
import { setTestFallbackLocale } from "../i18n/localeState";

// Set legacy test suite default locale to Simplified Chinese so historical assertions pass
setTestFallbackLocale("zh-Hans");
