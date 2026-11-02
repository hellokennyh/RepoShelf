import { describe, it, expect } from "vitest";
import * as fs from "fs";
import * as path from "path";

describe("S1: NSIS Multilingual Installer Configuration and Integrity Suite", () => {
  const tauriConfPath = path.resolve(__dirname, "../../src-tauri/tauri.conf.json");
  const rawContent = fs.readFileSync(tauriConfPath, "utf-8");
  const tauriConf = JSON.parse(rawContent);

  it("AC01 & AC03: bundle.targets is strictly restricted to ['nsis'] only", () => {
    expect(tauriConf.bundle).toBeDefined();
    expect(tauriConf.bundle.targets).toEqual(["nsis"]);
    expect(tauriConf.bundle.targets).not.toContain("msi");
    expect(tauriConf.bundle.targets).not.toContain("all");
  });

  it("AC02: bundle.windows.nsis specifies English, SimpChinese, and TradChinese with English fallback", () => {
    const nsis = tauriConf.bundle?.windows?.nsis;
    expect(nsis).toBeDefined();
    expect(nsis.languages).toEqual(["English", "SimpChinese", "TradChinese"]);
    expect(nsis.languages[0]).toBe("English");
    expect(nsis.displayLanguageSelector).toBe(true);
  });

  it("AC04: Application identity, productName, and bundle active flag are strictly preserved", () => {
    expect(tauriConf.identifier).toBe("com.reposhelf.desktop");
    expect(tauriConf.productName).toBe("RepoShelf");
    expect(tauriConf.bundle.active).toBe(true);
  });

  it("AC01 & AC06: NSIS language files exist in the local NSIS installation", () => {
    const localAppData = process.env.LOCALAPPDATA;
    if (localAppData) {
      const nsisLangDir = path.join(localAppData, "tauri", "NSIS", "Contrib", "Language files");
      if (fs.existsSync(nsisLangDir)) {
        expect(fs.existsSync(path.join(nsisLangDir, "English.nlf"))).toBe(true);
        expect(fs.existsSync(path.join(nsisLangDir, "SimpChinese.nlf"))).toBe(true);
        expect(fs.existsSync(path.join(nsisLangDir, "TradChinese.nlf"))).toBe(true);
        expect(fs.existsSync(path.join(nsisLangDir, "English.nsh"))).toBe(true);
        expect(fs.existsSync(path.join(nsisLangDir, "SimpChinese.nsh"))).toBe(true);
        expect(fs.existsSync(path.join(nsisLangDir, "TradChinese.nsh"))).toBe(true);
      }
    }
  });

  it("AC06: AppData and SQLite data path contracts are unchanged", () => {
    expect(tauriConf.identifier).toBe("com.reposhelf.desktop");
    // Ensure no destructive custom hooks or wipe scripts are configured in nsis
    const nsis = tauriConf.bundle?.windows?.nsis;
    expect(nsis.installerHooks).toBeUndefined();
  });
});
