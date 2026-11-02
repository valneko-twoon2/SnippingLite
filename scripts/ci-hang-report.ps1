# CI のテストが出力を止めたとき（止まったテストの名前は、最後の [開始] の行）、そのときの様子を
# ログに出す。テストのプロセスの見えている窓（ダイアログは中の文字も）、プロセスの状態、デスクトップの
# 画像。`-Close` を付けると、見つかったダイアログを閉じる（ダイアログが開いたままテストが戻らない
# 止まり方なら、これでテストが失敗して先へ進む）。ci-cargo-test.ps1 の見張りが呼ぶ。
# 使い方: . scripts/ci-hang-report.ps1; Write-HangReport -Name screen -Close
if (-not ('HangWindows' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class HangWindows {
    delegate bool EnumProc(IntPtr hwnd, IntPtr lParam);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc proc, IntPtr lParam);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr hwnd, EnumProc proc, IntPtr lParam);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd, StringBuilder text, int max);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] static extern bool IsHungAppWindow(IntPtr hwnd);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint message, IntPtr wParam, IntPtr text, uint flags, uint timeout, out IntPtr result);

    // 題名。相手が固まっていても待ちすぎないよう、時間を区切って WM_GETTEXT を送る。
    static string Text(IntPtr hwnd) {
        IntPtr buffer = Marshal.AllocHGlobal(1024);
        try {
            Marshal.WriteInt16(buffer, 0);
            IntPtr result;
            if (SendMessageTimeout(hwnd, 0x000D, (IntPtr)256, buffer, 0x2 /* SMTO_ABORTIFHUNG */, 500, out result) == IntPtr.Zero) return "(読めない)";
            return Marshal.PtrToStringUni(buffer) ?? "";
        } finally { Marshal.FreeHGlobal(buffer); }
    }

    static string Class(IntPtr hwnd) {
        StringBuilder text = new StringBuilder(128);
        GetClassName(hwnd, text, text.Capacity);
        return text.ToString();
    }

    // `pids` のプロセスの最上位の窓を 1 行ずつ。ダイアログ（#32770）は文字のある子の窓も添える。
    public static List<string> Describe(HashSet<uint> pids, bool close) {
        List<string> lines = new List<string>();
        EnumWindows(delegate (IntPtr hwnd, IntPtr unused) {
            uint pid;
            GetWindowThreadProcessId(hwnd, out pid);
            if (!pids.Contains(pid)) return true;
            bool visible = IsWindowVisible(hwnd);
            string cls = Class(hwnd);
            if (!visible && cls != "#32770") return true;
            string line = string.Format("pid {0} {1:x} {2} {3} visible={4} hung={5}", pid, (long)hwnd, cls, Text(hwnd), visible, IsHungAppWindow(hwnd));
            if (cls == "#32770") {
                List<string> children = new List<string>();
                EnumChildWindows(hwnd, delegate (IntPtr child, IntPtr unused2) {
                    string text = Text(child);
                    if (text.Trim().Length > 0 && children.Count < 40) children.Add(Class(child) + " " + text);
                    return true;
                }, IntPtr.Zero);
                line += " [" + string.Join(", ", children) + "]";
                if (close) PostMessage(hwnd, 0x0010 /* WM_CLOSE */, IntPtr.Zero, IntPtr.Zero);
            }
            lines.Add(line);
            return true;
        }, IntPtr.Zero);
        return lines;
    }
}
'@
}

# テストのプロセス: cargo が作るテストの exe（target\debug\deps）と、E2E がテストするアプリの写し。
function Get-TestProcess {
    Get-Process -ErrorAction SilentlyContinue | Where-Object {
        $_.Path -and ($_.Path -like '*\target\debug\deps\*' -or $_.Path -like '*\snippinglite-e2e-exe\*')
    }
}

# 様子をログに出し、注釈に添えられるよう、同じ内容を `%0A` でつないだ 1 つの文字列で返す。
function Write-HangReport {
    param([string]$Name = 'hang', [switch]$Close)
    $processes = @(Get-TestProcess)
    $report = [System.Collections.Generic.List[string]]::new()
    foreach ($process in $processes) {
        $report.Add("プロセス $($process.Id) $($process.ProcessName) 応答=$($process.Responding) CPU=$([math]::Round($process.CPU, 1)) s スレッド=$($process.Threads.Count)")
    }
    $ids = [System.Collections.Generic.HashSet[uint32]]::new()
    foreach ($process in $processes) { $null = $ids.Add([uint32]$process.Id) }
    foreach ($line in [HangWindows]::Describe($ids, [bool]$Close)) { $report.Add($line) }
    [Console]::WriteLine('::group::出力が止まった: テストのプロセスの様子')
    foreach ($line in $report) { [Console]::WriteLine($line) }
    [Console]::WriteLine('::endgroup::')
    try { & (Join-Path $PSScriptRoot 'ci-screenshot.ps1') -Name $Name | ForEach-Object { [Console]::WriteLine($_) } }
    catch { [Console]::WriteLine("画像を残せなかった: $_") }
    return ($report -join '%0A')
}
