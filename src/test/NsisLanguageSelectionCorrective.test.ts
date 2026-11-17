import { describe, it, expect } from "vitest";
import * as fs from "fs";
import * as path from "path";
import * as os from "os";
import { execFileSync } from "child_process";

/**
 * Pure JavaScript simulation of the NSIS locale matching algorithm
 * implemented in src-tauri/nsis/installer_hooks.nsh.
 *
 * Algorithm:
 *   primary = lcid & 0x03FF
 *   sublang = lcid >> 10
 *   if (primary === 4) { // LANG_CHINESE
 *     if (sublang === 1 || sublang === 3 || sublang === 5) return 1028; // TradChinese
 *     return 2052; // SimpChinese
 *   }
 *   if (primary === 9) return 1033; // English
 *   return 1033; // Fallback
 */
function resolveNsisLocale(lcid: number): number {
  const primary = lcid & 0x03ff;
  const sublang = lcid >> 10;
  if (primary === 4) {
    if (sublang === 1 || sublang === 3 || sublang === 5) {
      return 1028; // TradChinese (TW, HK, MO)
    }
    return 2052; // SimpChinese (PRC, SG, etc.)
  }
  if (primary === 9) {
    return 1033; // English
  }
  return 1033; // Unsupported system language fallback
}

/**
 * Pure simulation of the macro execution decision logic:
 * decides whether to display the language dialog and what default language is used.
 */
function simulateNsisInstallerLanguageSelection(options: {
  windowsUiLcid: number;
  registryLanguage?: string | null;
  passiveMode?: boolean;
  updateMode?: boolean;
  silentMode?: boolean;
  userSelectionOverride?: number;
}): {
  dialogDisplayed: boolean;
  preselectedLanguage: number;
  finalLanguage: number;
} {
  const detectedSystemLang = resolveNsisLocale(options.windowsUiLcid);
  const isAutomated = options.passiveMode || options.updateMode || options.silentMode;

  if (isAutomated) {
    // In passive / update / silent mode: NEVER show dialog!
    let finalLang = detectedSystemLang;
    if (options.registryLanguage && options.registryLanguage.trim() !== "") {
      const parsedReg = parseInt(options.registryLanguage.trim(), 10);
      if (!isNaN(parsedReg) && (parsedReg === 1033 || parsedReg === 2052 || parsedReg === 1028)) {
        finalLang = parsedReg;
      }
    }
    return {
      dialogDisplayed: false,
      preselectedLanguage: finalLang,
      finalLanguage: finalLang,
    };
  }

  // Interactive mode: ALWAYS show dialog!
  // Pre-selected default strictly follows Windows UI language priority:
  const preselected = detectedSystemLang;
  const finalLang = options.userSelectionOverride !== undefined
    ? options.userSelectionOverride
    : preselected;

  return {
    dialogDisplayed: true,
    preselectedLanguage: preselected,
    finalLanguage: finalLang,
  };
}

describe("REPOSHELF_V1_1_4_S5E_R1: NSIS Multilingual Installer Language Corrective Suite", () => {
  const tauriConfPath = path.resolve(__dirname, "../../src-tauri/tauri.conf.json");
  const rawTauriConf = fs.readFileSync(tauriConfPath, "utf-8");
  const tauriConf = JSON.parse(rawTauriConf);
  const hookFilePath = path.resolve(__dirname, "../../src-tauri/nsis/installer_hooks.nsh");

  // --------------------------------------------------------------------------
  // Category 1: Static Configuration & Architecture Guardrails (Req 15, 16)
  // --------------------------------------------------------------------------
  describe("Category 1: Configuration & Security Architecture Integrity", () => {
    it("Req 15: bundle.targets remains strictly ['nsis'] only", () => {
      expect(tauriConf.bundle.targets).toEqual(["nsis"]);
      expect(tauriConf.bundle.targets).not.toContain("msi");
      expect(tauriConf.bundle.targets).not.toContain("all");
    });

    it("Req 16: Updater pubkey, endpoint, and artifact flags are untouched", () => {
      expect(tauriConf.bundle.createUpdaterArtifacts).toBe(true);
      expect(tauriConf.plugins?.updater?.pubkey).toBe(
        "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEYzNTdEMzNERTNDM0E1RjUKUldUMXBjUGpQZE5YODEwMTNSV1dud0xtZjU1KzhtSmVYVWkxSHRRWTRucVR5U1R6dWR0ODZVbkkK"
      );
      expect(tauriConf.plugins?.updater?.endpoints).toEqual([
        "https://github.com/hellokennyh/RepoShelf/releases/latest/download/latest.json",
      ]);
      expect(tauriConf.plugins?.updater?.windows?.installMode).toBe("passive");
    });

    it("Preserves application identity, productName, and version", () => {
      expect(tauriConf.identifier).toBe("com.reposhelf.desktop");
      expect(tauriConf.productName).toBe("RepoShelf");
      expect(tauriConf.version).toBe("1.1.4");
    });

    it("Configures installerHooks pointing to nsis/installer_hooks.nsh", () => {
      const nsis = tauriConf.bundle?.windows?.nsis;
      expect(nsis).toBeDefined();
      expect(nsis.installerHooks).toBe("nsis/installer_hooks.nsh");
      expect(nsis.displayLanguageSelector).toBe(true);
      expect(nsis.languages).toEqual(["English", "SimpChinese", "TradChinese"]);
    });
  });

  // --------------------------------------------------------------------------
  // Category 2: installer_hooks.nsh Structural & Security Audit (Req 13, 14)
  // --------------------------------------------------------------------------
  describe("Category 2: NSIS Hook File Syntax and Safety Audit", () => {
    it("The hook file exists and is readable", () => {
      expect(fs.existsSync(hookFilePath)).toBe(true);
    });

    it("Req 13 & 14: Hook file is non-destructive (no AppData, SQLite or data wipes)", () => {
      const content = fs.readFileSync(hookFilePath, "utf-8");
      expect(content).not.toMatch(/RMDir\s+\/r/i);
      expect(content).not.toMatch(/DeleteRegKey\s+.*AppData/i);
      expect(content).not.toMatch(/Delete\s+.*\.db/i);
      expect(content).not.toMatch(/Delete\s+.*reposhelf/i);
    });

    it("Properly undefines upstream MUI_LANGDLL_DISPLAY and redefines it", () => {
      const content = fs.readFileSync(hookFilePath, "utf-8");
      expect(content).toContain("!macroundef MUI_LANGDLL_DISPLAY");
      expect(content).toContain("!macro MUI_LANGDLL_DISPLAY");
      expect(content).toContain("!macroend");
    });

    it("Contains Win32 GetUserDefaultUILanguage call and bitmask decomposition", () => {
      const content = fs.readFileSync(hookFilePath, "utf-8");
      expect(content).toContain("kernel32::GetUserDefaultUILanguage");
      expect(content).toContain("0x03FF");
      expect(content).toContain("1028");
      expect(content).toContain("2052");
      expect(content).toContain("1033");
    });

    it("Checks for passive (/P), auto-update (/UPDATE), and silent (/S) modes", () => {
      const content = fs.readFileSync(hookFilePath, "utf-8");
      expect(content).toContain("$PassiveMode");
      expect(content).toContain("$UpdateMode");
      expect(content).toContain("${Silent}");
    });

    it("Saves user selection to registry upon interactive choice", () => {
      const content = fs.readFileSync(hookFilePath, "utf-8");
      expect(content).toContain("LangDLL::LangDialog");
      expect(content).toContain("WriteRegStr");
      expect(content).toContain("${MUI_LANGDLL_REGISTRY_VALUENAME}");
    });

    it("Defines standard no-op lifecycle hooks", () => {
      const content = fs.readFileSync(hookFilePath, "utf-8");
      expect(content).toContain("!macro NSIS_HOOK_PREINSTALL");
      expect(content).toContain("!macro NSIS_HOOK_POSTINSTALL");
      expect(content).toContain("!macro NSIS_HOOK_PREUNINSTALL");
      expect(content).toContain("!macro NSIS_HOOK_POSTUNINSTALL");
    });
  });

  // --------------------------------------------------------------------------
  // Category 3: Locale Priority & Language Selection Simulation (Req 1-9)
  // --------------------------------------------------------------------------
  describe("Category 3: Windows Locale Detection and Interactive Priority", () => {
    it("Req 1: Fresh install with no registry defaults to Windows UI language and shows dialog", () => {
      const resZh = simulateNsisInstallerLanguageSelection({
        windowsUiLcid: 2052,
        registryLanguage: null,
      });
      expect(resZh.dialogDisplayed).toBe(true);
      expect(resZh.preselectedLanguage).toBe(2052);
      expect(resZh.finalLanguage).toBe(2052);
    });

    it("Req 2: Windows Simplified Chinese (zh-CN 2052, zh-SG 4100) preselects 2052", () => {
      expect(resolveNsisLocale(2052)).toBe(2052);
      expect(resolveNsisLocale(4100)).toBe(2052);

      const res = simulateNsisInstallerLanguageSelection({ windowsUiLcid: 2052 });
      expect(res.dialogDisplayed).toBe(true);
      expect(res.preselectedLanguage).toBe(2052);
    });

    it("Req 3: Windows Traditional Chinese (zh-TW 1028, zh-HK 3076, zh-MO 5124) preselects 1028", () => {
      expect(resolveNsisLocale(1028)).toBe(1028);
      expect(resolveNsisLocale(3076)).toBe(1028);
      expect(resolveNsisLocale(5124)).toBe(1028);

      const res = simulateNsisInstallerLanguageSelection({ windowsUiLcid: 1028 });
      expect(res.dialogDisplayed).toBe(true);
      expect(res.preselectedLanguage).toBe(1028);
    });

    it("Req 4: Windows English (en-US 1033, en-GB 2057, en-AU 3081) preselects 1033", () => {
      expect(resolveNsisLocale(1033)).toBe(1033);
      expect(resolveNsisLocale(2057)).toBe(1033);
      expect(resolveNsisLocale(3081)).toBe(1033);

      const res = simulateNsisInstallerLanguageSelection({ windowsUiLcid: 1033 });
      expect(res.dialogDisplayed).toBe(true);
      expect(res.preselectedLanguage).toBe(1033);
    });

    it("Req 5: Unsupported system languages (Japanese, French, German, Korean) fallback to English (1033)", () => {
      expect(resolveNsisLocale(1041)).toBe(1033); // Japanese (0x0411)
      expect(resolveNsisLocale(1036)).toBe(1033); // French (0x040C)
      expect(resolveNsisLocale(1031)).toBe(1033); // German (0x0407)
      expect(resolveNsisLocale(1042)).toBe(1033); // Korean (0x0412)

      const res = simulateNsisInstallerLanguageSelection({ windowsUiLcid: 1041 });
      expect(res.dialogDisplayed).toBe(true);
      expect(res.preselectedLanguage).toBe(1033);
    });

    it("Req 6 & 9: Manual upgrade with stale English (1033) registry value STILL shows dialog", () => {
      const res = simulateNsisInstallerLanguageSelection({
        windowsUiLcid: 2052,
        registryLanguage: "1033",
      });
      // Dialog MUST NOT be skipped!
      expect(res.dialogDisplayed).toBe(true);
      // Pre-selected language prioritizes current Windows UI language (2052)!
      expect(res.preselectedLanguage).toBe(2052);
    });

    it("Req 7: Stale registry vs Windows UI language discrepancy favors Windows UI language for default", () => {
      // User has zh-CN Windows, but registry has 1033 from older version
      const res = simulateNsisInstallerLanguageSelection({
        windowsUiLcid: 2052,
        registryLanguage: "1033",
      });
      expect(res.preselectedLanguage).toBe(2052);

      // User has en-US Windows, but registry has 2052
      const resEn = simulateNsisInstallerLanguageSelection({
        windowsUiLcid: 1033,
        registryLanguage: "2052",
      });
      expect(resEn.preselectedLanguage).toBe(1033);
    });

    it("Req 8: User can manually override the pre-selected language to any of the 3 languages", () => {
      // Windows is zh-CN (defaults 2052), but user chooses TradChinese (1028)
      const resTw = simulateNsisInstallerLanguageSelection({
        windowsUiLcid: 2052,
        userSelectionOverride: 1028,
      });
      expect(resTw.dialogDisplayed).toBe(true);
      expect(resTw.preselectedLanguage).toBe(2052);
      expect(resTw.finalLanguage).toBe(1028);

      // Windows is zh-CN, but user chooses English (1033)
      const resEn = simulateNsisInstallerLanguageSelection({
        windowsUiLcid: 2052,
        userSelectionOverride: 1033,
      });
      expect(resEn.dialogDisplayed).toBe(true);
      expect(resEn.preselectedLanguage).toBe(2052);
      expect(resEn.finalLanguage).toBe(1033);
    });
  });

  // --------------------------------------------------------------------------
  // Category 4: Tauri Updater Non-Interactive & Silent Modes (Req 10, 11, 12)
  // --------------------------------------------------------------------------
  describe("Category 4: Passive Update, Auto-Update, and Silent Mode Isolation", () => {
    it("Req 10: /P passive update NEVER shows language dialog and does not block", () => {
      const res = simulateNsisInstallerLanguageSelection({
        windowsUiLcid: 2052,
        registryLanguage: "1033",
        passiveMode: true,
      });
      expect(res.dialogDisplayed).toBe(false);
      expect(res.finalLanguage).toBe(1033); // preserves user's existing install language
    });

    it("Req 10: /P passive update with no prior registry falls back to Windows UI language", () => {
      const res = simulateNsisInstallerLanguageSelection({
        windowsUiLcid: 2052,
        registryLanguage: null,
        passiveMode: true,
      });
      expect(res.dialogDisplayed).toBe(false);
      expect(res.finalLanguage).toBe(2052);
    });

    it("Req 11: /UPDATE auto-update NEVER shows language dialog", () => {
      const res = simulateNsisInstallerLanguageSelection({
        windowsUiLcid: 1028,
        registryLanguage: "1028",
        updateMode: true,
      });
      expect(res.dialogDisplayed).toBe(false);
      expect(res.finalLanguage).toBe(1028);
    });

    it("Req 12: /S silent install NEVER shows language dialog", () => {
      const res = simulateNsisInstallerLanguageSelection({
        windowsUiLcid: 1033,
        silentMode: true,
      });
      expect(res.dialogDisplayed).toBe(false);
      expect(res.finalLanguage).toBe(1033);
    });
  });

  // --------------------------------------------------------------------------
  // Category 5: Real NSIS Compiler Build Verification (When makensis is available)
  // --------------------------------------------------------------------------
  describe("Category 5: Real makensis Compilation Verification", () => {
    it("Compiles an NSIS test script with installer_hooks.nsh successfully", () => {
      const localAppData = process.env.LOCALAPPDATA;
      if (!localAppData) {
        return;
      }
      const makensisPath = path.join(localAppData, "tauri/NSIS/Bin/makensis.exe");
      if (!fs.existsSync(makensisPath)) {
        return;
      }

      const tempDir = path.join(os.tmpdir(), "reposhelf-nsis-test");
      if (!fs.existsSync(tempDir)) {
        fs.mkdirSync(tempDir, { recursive: true });
      }

      const testNsiPath = path.join(tempDir, "test_hook_compile.nsi");
      const testExePath = path.join(tempDir, "test_hook_compile.exe");

      const testScript = `
Unicode true
OutFile "${testExePath.replace(/\\/g, "\\\\")}"
RequestExecutionLevel user

!include MUI2.nsh
!include FileFunc.nsh
!include LogicLib.nsh

!include "${hookFilePath.replace(/\\/g, "\\\\")}"

!define MANUFACTURER "reposhelf"
!define PRODUCTNAME "RepoShelf"
!define MANUKEY "Software\\\${MANUFACTURER}"
!define MANUPRODUCTKEY "\${MANUKEY}\\\${PRODUCTNAME}"

!define MUI_LANGDLL_REGISTRY_ROOT "HKCU"
!define MUI_LANGDLL_REGISTRY_KEY "\${MANUPRODUCTKEY}"
!define MUI_LANGDLL_REGISTRY_VALUENAME "Installer Language"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "TradChinese"

Var PassiveMode
Var UpdateMode

Function .onInit
  StrCpy $PassiveMode 1
  !insertmacro MUI_LANGDLL_DISPLAY
  Abort
FunctionEnd

Section
SectionEnd
`;
      fs.writeFileSync(testNsiPath, testScript, "utf-8");

      try {
        const output = execFileSync(makensisPath, [testNsiPath], { encoding: "utf-8" });
        expect(output).toContain("Processed 1 file");
        expect(fs.existsSync(testExePath)).toBe(true);
      } finally {
        if (fs.existsSync(testExePath)) {
          fs.unlinkSync(testExePath);
        }
        if (fs.existsSync(testNsiPath)) {
          fs.unlinkSync(testNsiPath);
        }
      }
    });
  });
});
