#include <windows.h>
#include <tlhelp32.h>
#include <shellapi.h>

#include <string>
#include <vector>

namespace {
constexpr wchar_t kProperty[] = L"OpenFader.MainWindow.2DFA9F36-4697-4B79-88C8-F4883ED0B387";
constexpr wchar_t kMessage[] = L"OpenFader.UpdateQuit.72F47CF8-58B0-4C0D-848A-B1CB33E768F7";
constexpr DWORD_PTR kAck = 0x4f465131;

std::vector<BYTE> Sid(HANDLE process) {
  HANDLE token = nullptr;
  if (!OpenProcessToken(process, TOKEN_QUERY, &token)) return {};
  DWORD size = 0;
  GetTokenInformation(token, TokenUser, nullptr, 0, &size);
  std::vector<BYTE> result(size);
  if (!size || !GetTokenInformation(token, TokenUser, result.data(), size, &size)) result.clear();
  CloseHandle(token);
  return result;
}

bool SameUser(HANDLE process, const std::vector<BYTE>& sid) {
  auto other = Sid(process);
  return !other.empty() && !sid.empty() && EqualSid(
      reinterpret_cast<const TOKEN_USER*>(other.data())->User.Sid,
      reinterpret_cast<const TOKEN_USER*>(sid.data())->User.Sid);
}

struct Processes { std::vector<DWORD> app; bool host = false; bool capture = false; };
Processes FindProcesses(const std::wstring& directory, const std::vector<BYTE>& sid) {
  Processes found;
  HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
  if (snapshot == INVALID_HANDLE_VALUE) { found.host = true; return found; }
  PROCESSENTRY32W entry{};
  entry.dwSize = sizeof(entry);
  if (Process32FirstW(snapshot, &entry)) do {
    if (_wcsicmp(entry.szExeFile, L"openfader.exe") &&
        _wcsicmp(entry.szExeFile, L"openfader-serve.exe") &&
        _wcsicmp(entry.szExeFile, L"openfader-capture-helper.exe")) continue;
    HANDLE process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, entry.th32ProcessID);
    if (!process) continue;
    wchar_t path[32768];
    DWORD length = static_cast<DWORD>(sizeof(path) / sizeof(path[0]));
    if (QueryFullProcessImageNameW(process, 0, path, &length) && SameUser(process, sid)) {
      if (!_wcsicmp(path, (directory + L"\\openfader.exe").c_str())) found.app.push_back(entry.th32ProcessID);
      if (!_wcsicmp(path, (directory + L"\\openfader-serve.exe").c_str())) found.host = true;
      if (!_wcsicmp(path, (directory + L"\\openfader-capture-helper.exe").c_str())) found.capture = true;
    }
    CloseHandle(process);
  } while (Process32NextW(snapshot, &entry));
  CloseHandle(snapshot);
  return found;
}
struct Search { const std::vector<DWORD>* ids; HWND window = nullptr; };
BOOL CALLBACK FindWindow(HWND window, LPARAM value) {
  auto* search = reinterpret_cast<Search*>(value);
  if (!GetPropW(window, kProperty)) return TRUE;
  DWORD id = 0;
  GetWindowThreadProcessId(window, &id);
  for (DWORD candidate : *search->ids) if (candidate == id) { search->window = window; return FALSE; }
  return TRUE;
}
}  // namespace

int WINAPI wWinMain(HINSTANCE, HINSTANCE, PWSTR, int) {
  int argc = 0;
  LPWSTR* argv = CommandLineToArgvW(GetCommandLineW(), &argc);
  if (!argv || argc != 2) { if (argv) LocalFree(argv); return 2; }
  wchar_t full[32768];
  DWORD n = GetFullPathNameW(argv[1], static_cast<DWORD>(sizeof(full) / sizeof(full[0])), full, nullptr);
  LocalFree(argv);
  if (!n || n >= sizeof(full) / sizeof(full[0])) return 2;
  std::wstring dir(full);
  while (dir.size() > 3 && (dir.back() == L'\\' || dir.back() == L'/')) dir.pop_back();
  const auto sid = Sid(GetCurrentProcess());
  if (sid.empty()) return 2;
  auto processes = FindProcesses(dir, sid);
  if (processes.app.empty() && !processes.host && !processes.capture) return 0;
  // A capture process without its host cannot be safely coordinated here.
  if (processes.app.empty() && !processes.host) return 2;
  if (processes.app.empty()) {
    // Reopen the local UI so its normal authenticated shutdown can run.
    const auto exe = dir + L"\\openfader.exe";
    if (reinterpret_cast<INT_PTR>(ShellExecuteW(nullptr, L"open", exe.c_str(), nullptr, dir.c_str(), SW_SHOWNORMAL)) <= 32) return 2;
  }
  const ULONGLONG ready = GetTickCount64() + 15000;
  HWND window = nullptr;
  do {
    processes = FindProcesses(dir, sid);
    Search search{&processes.app};
    EnumWindows(FindWindow, reinterpret_cast<LPARAM>(&search));
    window = search.window;
    if (!window) Sleep(250);
  } while (!window && GetTickCount64() < ready);
  if (!window) return 2;
  // Give a relaunched UI time to attach its method handler and local host.
  if (processes.host) Sleep(3000);
  const UINT message = RegisterWindowMessageW(kMessage);
  DWORD_PTR answer = 0;
  if (!message || !SendMessageTimeoutW(window, message, 0, 0, SMTO_ABORTIFHUNG, 3000, &answer) || answer != kAck) return 2;
  const ULONGLONG deadline = GetTickCount64() + 60000;
  do {
    processes = FindProcesses(dir, sid);
    if (processes.app.empty() && !processes.host && !processes.capture) return 10;
    Sleep(250);
  } while (GetTickCount64() < deadline);
  return 2;
}
