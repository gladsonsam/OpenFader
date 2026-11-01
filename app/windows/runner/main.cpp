#include <flutter/dart_project.h>
#include <flutter/flutter_view_controller.h>
#include <windows.h>
#include <sddl.h>

#include <string>
#include <vector>

#include "flutter_window.h"
#include "utils.h"

namespace {
constexpr wchar_t kInstanceWindow[] =
    L"OpenFader.MainWindow.2DFA9F36-4697-4B79-88C8-F4883ED0B387";

std::wstring UserSid(DWORD process_id) {
  HANDLE process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, process_id);
  if (!process) return {};
  HANDLE token = nullptr;
  if (!OpenProcessToken(process, TOKEN_QUERY, &token)) {
    CloseHandle(process);
    return {};
  }
  CloseHandle(process);
  DWORD length = 0;
  GetTokenInformation(token, TokenUser, nullptr, 0, &length);
  std::vector<BYTE> data(length);
  std::wstring sid;
  if (length != 0 && GetTokenInformation(token, TokenUser, data.data(), length, &length)) {
    LPWSTR text = nullptr;
    if (ConvertSidToStringSidW(reinterpret_cast<TOKEN_USER*>(data.data())->User.Sid, &text)) {
      sid = text;
      LocalFree(text);
    }
  }
  CloseHandle(token);
  return sid;
}

struct ExistingWindow {
  std::wstring sid;
  HWND window = nullptr;
};

BOOL CALLBACK FindExistingWindow(HWND window, LPARAM parameter) {
  if (!GetPropW(window, kInstanceWindow)) return TRUE;
  auto* existing = reinterpret_cast<ExistingWindow*>(parameter);
  DWORD process_id = 0;
  GetWindowThreadProcessId(window, &process_id);
  if (UserSid(process_id) != existing->sid) return TRUE;
  existing->window = window;
  return FALSE;
}

// Hold the mutex for the complete process lifetime. A launch racing startup
// waits for its window; if the first process dies, the waiter becomes primary.
class SingleInstance {
 public:
  ~SingleInstance() {
    if (owns_) ReleaseMutex(mutex_);
    if (mutex_) CloseHandle(mutex_);
  }

  bool Acquire() {
    const auto sid = UserSid(GetCurrentProcessId());
    if (sid.empty()) return Fail();
    const auto name = L"Local\\OpenFader.Application." + sid;
    mutex_ = CreateMutexW(nullptr, TRUE, name.c_str());
    if (!mutex_) return Fail();
    if (GetLastError() != ERROR_ALREADY_EXISTS) {
      owns_ = true;
      return true;
    }
    const ULONGLONG deadline = GetTickCount64() + 10000;
    while (GetTickCount64() < deadline) {
      ExistingWindow existing{sid, nullptr};
      EnumWindows(FindExistingWindow, reinterpret_cast<LPARAM>(&existing));
      if (existing.window) {
        // The new foreground process may grant focus to the existing one.
        DWORD process_id = 0;
        GetWindowThreadProcessId(existing.window, &process_id);
        AllowSetForegroundWindow(process_id);
        ShowWindowAsync(existing.window,
                        IsIconic(existing.window) ? SW_RESTORE : SW_SHOW);
        SetForegroundWindow(existing.window);
        return false;
      }
      const DWORD result = WaitForSingleObject(mutex_, 100);
      if (result == WAIT_OBJECT_0 || result == WAIT_ABANDONED) {
        owns_ = true;
        return true;
      }
      if (result == WAIT_FAILED) return Fail();
    }
    return Fail();
  }

 private:
  bool Fail() {
    MessageBoxW(nullptr,
                L"OpenFader could not activate its existing window. It may still be starting. Please try opening it again.",
                L"OpenFader", MB_OK | MB_ICONINFORMATION);
    return false;
  }
  HANDLE mutex_ = nullptr;
  bool owns_ = false;
};
}  // namespace

int APIENTRY wWinMain(_In_ HINSTANCE instance, _In_opt_ HINSTANCE prev,
                      _In_ wchar_t *command_line, _In_ int show_command) {
  SingleInstance single_instance;
  if (!single_instance.Acquire()) return EXIT_SUCCESS;
  // Attach to console when present (e.g., 'flutter run') or create a
  // new console when running with a debugger.
  if (!::AttachConsole(ATTACH_PARENT_PROCESS) && ::IsDebuggerPresent()) {
    CreateAndAttachConsole();
  }

  // Initialize COM, so that it is available for use in the library and/or
  // plugins.
  ::CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);

  flutter::DartProject project(L"data");

  std::vector<std::string> command_line_arguments =
      GetCommandLineArguments();

  project.set_dart_entrypoint_arguments(std::move(command_line_arguments));

  FlutterWindow window(project);
  Win32Window::Point origin(10, 10);
  Win32Window::Size size(1280, 720);
  if (!window.Create(L"OpenFader", origin, size)) {
    ::CoUninitialize();
    return EXIT_FAILURE;
  }
  if (!SetPropW(window.GetHandle(), kInstanceWindow,
                reinterpret_cast<HANDLE>(static_cast<INT_PTR>(1)))) {
    ::CoUninitialize();
    return EXIT_FAILURE;
  }
  window.SetQuitOnClose(true);

  ::MSG msg;
  while (::GetMessage(&msg, nullptr, 0, 0)) {
    ::TranslateMessage(&msg);
    ::DispatchMessage(&msg);
  }

  ::CoUninitialize();
  return EXIT_SUCCESS;
}
