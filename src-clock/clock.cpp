// duri_clock.dll: runs inside explorer.exe and replaces the text of the
// taskbar clock with lines sent by Duri. See docs/ARCHITECTURE.md › Taskbar
// clock for the whole picture; src-tauri/src/taskbar_clock.rs is the other end.
//
// How it gets there:
//   1. Duri installs a WH_CALLWNDPROC hook on the taskbar thread, which makes
//      Windows load this DLL into explorer, and sends the attach message.
//   2. We create a message-only window (the command channel) and connect to
//      XAML Diagnostics, the API behind Visual Studio's Live Visual Tree. It
//      reports every element of the taskbar's XAML tree to us.
//   3. When the clock's time TextBlock shows up we remember it. On a "set"
//      command we collapse the panel holding the time and date and put our
//      own TextBlock next to it; "revert" removes ours and restores theirs.
//
// Nothing here hooks functions or depends on private symbols: if a Windows
// update renames the clock's elements we never find them and do nothing.
//
// The DLL never unloads (XAML keeps pointers into it). Reverted, it is inert
// until explorer restarts.

#include <unknwn.h>
#include <windows.h>
#include <ocidl.h>
#include <xamlOM.h>

#include <algorithm>
#include <atomic>
#include <cstdarg>
#include <cstdio>
#include <string>
#include <vector>

#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.UI.Core.h>
#include <winrt/Windows.UI.Text.h>
#include <winrt/Windows.UI.Xaml.h>
#include <winrt/Windows.UI.Xaml.Controls.h>
#include <winrt/Windows.UI.Xaml.Data.h>
#include <winrt/Windows.UI.Xaml.Media.h>

using namespace winrt::Windows::UI::Xaml;
using namespace winrt::Windows::UI::Xaml::Controls;
using namespace winrt::Windows::UI::Xaml::Media;
using winrt::Windows::UI::Core::CoreDispatcherPriority;

namespace {

// {6B0C1F0A-6C0B-4B58-9D3E-1D0E5B7B8A11}
const CLSID CLSID_DuriClockTap = {0x6b0c1f0a, 0x6c0b, 0x4b58, {0x9d, 0x3e, 0x1d, 0x0e, 0x5b, 0x7b, 0x8a, 0x11}};

// Keep in sync with src-tauri/src/taskbar_clock.rs.
const wchar_t ATTACH_MESSAGE[] = L"DuriClockAttach";
const wchar_t HOST_CLASS_PREFIX[] = L"DuriClockHost:";
const ULONG_PTR CMD_REVERT = 0;
// Payload: a SetHeader, then the text as NUL-terminated UTF-16 with '\n'
// between lines.
const ULONG_PTR CMD_SET_TEXT = 1;

struct SetHeader {
    // Duri's process id; the Windows clock comes back when it exits.
    DWORD owner;
    // 0 left, 1 center, 2 right.
    DWORD align;
    // Changes when the user edits the lines: forget the width kept so far.
    DWORD generation;
};

const UINT WM_HOST_INIT = WM_APP + 1;
const UINT_PTR OWNER_TIMER = 1;
const UINT OWNER_CHECK_MS = 2000;
// Right after explorer starts the taskbar window exists before its XAML
// does, so connecting is retried for a while.
const UINT_PTR CONNECT_TIMER = 2;
const UINT CONNECT_RETRY_MS = 1000;
const int CONNECT_ATTEMPTS = 120;

HMODULE g_module = nullptr;
HWND g_host = nullptr;

// Explorer has no console and Duri can't see in here, so the few things
// worth knowing go to %TEMP%\duri-clock.log.
void Log(const wchar_t* format, ...) {
    wchar_t message[1024];
    va_list args;
    va_start(args, format);
    _vsnwprintf_s(message, _TRUNCATE, format, args);
    va_end(args);

    wchar_t path[MAX_PATH];
    DWORD length = GetTempPathW(MAX_PATH, path);
    if (length == 0 || length > MAX_PATH - 20) return;
    wcscat_s(path, L"duri-clock.log");

    SYSTEMTIME now;
    GetLocalTime(&now);
    wchar_t line[1200];
    int chars = swprintf_s(line, L"%04d-%02d-%02d %02d:%02d:%02d %s\r\n", now.wYear, now.wMonth, now.wDay,
                           now.wHour, now.wMinute, now.wSecond, message);
    if (chars <= 0) return;
    char utf8[3600];
    int bytes = WideCharToMultiByte(CP_UTF8, 0, line, chars, utf8, sizeof(utf8), nullptr, nullptr);
    if (bytes <= 0) return;

    // Indexers and antivirus hold the file for a moment after each write.
    for (int attempt = 0; attempt < 10; attempt++) {
        HANDLE file = CreateFileW(path, FILE_APPEND_DATA, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                                  nullptr, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
        if (file != INVALID_HANDLE_VALUE) {
            // Start over rather than grow forever.
            LARGE_INTEGER size{};
            if (GetFileSizeEx(file, &size) && size.QuadPart > 256 * 1024) {
                SetFilePointer(file, 0, nullptr, FILE_BEGIN);
                SetEndOfFile(file);
            }
            DWORD written = 0;
            WriteFile(file, utf8, static_cast<DWORD>(bytes), &written, nullptr);
            CloseHandle(file);
            return;
        }
        Sleep(2);
    }
}

// ---------------------------------------------------------------------------
// Clock state. Touched on the taskbar's UI thread only.

struct Clock {
    // DateTimeStackPanel: holds the time and date TextBlocks. Windows keeps
    // toggling those two, so we hide the panel as a whole instead.
    FrameworkElement stack{nullptr};
    TextBlock time{nullptr};
    TextBlock ours{nullptr};
    bool hidden = false;
    Visibility stackVisibility = Visibility::Visible;
    int64_t visibilityToken = 0;
    // The SetHeader generation `ours` last measured its width for.
    DWORD generation = 0;
};

// One per taskbar. Entries are never dropped: a taskbar that went away
// leaves an unloaded element behind, which is harmless to touch.
std::vector<Clock> g_clocks;
std::wstring g_text;
DWORD g_align = 2;
DWORD g_generation = 0;
bool g_active = false;
HANDLE g_owner = nullptr;
DWORD g_ownerId = 0;

void Apply(Clock& clock) {
    if (!clock.stack || !clock.time) return;
    auto host = VisualTreeHelper::GetParent(clock.stack).try_as<Panel>();

    if (g_active) {
        if (!host) return;
        if (!clock.ours) {
            TextBlock ours;
            ours.Name(L"DuriClockText");
            ours.FontSize(clock.time.FontSize());
            ours.FontFamily(clock.time.FontFamily());
            ours.FontWeight(clock.time.FontWeight());
            // Bound, not copied: the taskbar swaps the brush with the theme.
            Data::Binding foreground;
            foreground.Source(clock.time);
            foreground.Path(PropertyPath{L"Foreground"});
            ours.SetBinding(TextBlock::ForegroundProperty(), foreground);
            ours.HorizontalAlignment(clock.stack.HorizontalAlignment());
            ours.VerticalAlignment(clock.stack.VerticalAlignment());
            ours.Margin(clock.stack.Margin());
            // Clicks and hover belong to the clock button underneath.
            ours.IsHitTestVisible(false);
            // Usage numbers change width every second, and the whole tray
            // would shift with them. So the text only ever grows: it keeps
            // the widest it has been until the lines are edited.
            ours.SizeChanged([](winrt::Windows::Foundation::IInspectable const& sender,
                                SizeChangedEventArgs const& args) {
                try {
                    auto element = sender.try_as<FrameworkElement>();
                    double width = args.NewSize().Width;
                    if (element && width > element.MinWidth()) element.MinWidth(width);
                } catch (...) {
                }
            });
            host.Children().Append(ours);
            clock.ours = ours;
            clock.generation = g_generation;
        }
        if (clock.generation != g_generation) {
            clock.generation = g_generation;
            clock.ours.MinWidth(0);
        }
        clock.ours.TextAlignment(g_align == 0   ? TextAlignment::Left
                                 : g_align == 1 ? TextAlignment::Center
                                                : TextAlignment::Right);
        // Two lines fit the taskbar at the clock's own size; a third needs
        // a slightly smaller font and tighter lines.
        bool three = std::count(g_text.begin(), g_text.end(), L'\n') >= 2;
        double size = clock.time.FontSize();
        clock.ours.FontSize(three ? size - 1 : size);
        clock.ours.LineStackingStrategy(three ? LineStackingStrategy::BlockLineHeight
                                              : LineStackingStrategy::MaxHeight);
        clock.ours.LineHeight(three ? size + 2 : 0);
        clock.ours.Text(g_text);
        if (!clock.hidden) {
            clock.stackVisibility = clock.stack.Visibility();
            clock.hidden = true;
            clock.stack.Visibility(Visibility::Collapsed);
            // If Windows shows the panel again, hide it again.
            clock.visibilityToken = clock.stack.RegisterPropertyChangedCallback(
                UIElement::VisibilityProperty(), [](DependencyObject const& sender, DependencyProperty const&) {
                    try {
                        auto element = sender.try_as<UIElement>();
                        if (g_active && element && element.Visibility() != Visibility::Collapsed) {
                            element.Visibility(Visibility::Collapsed);
                        }
                    } catch (...) {
                    }
                });
        }
    } else {
        if (clock.ours) {
            uint32_t index = 0;
            if (host && host.Children().IndexOf(clock.ours, index)) host.Children().RemoveAt(index);
            clock.ours = nullptr;
        }
        if (clock.hidden) {
            clock.hidden = false;
            clock.stack.UnregisterPropertyChangedCallback(UIElement::VisibilityProperty(), clock.visibilityToken);
            clock.stack.Visibility(clock.stackVisibility);
        }
    }
}

void ApplyAll() {
    for (auto& clock : g_clocks) {
        try {
            Apply(clock);
        } catch (winrt::hresult_error const& error) {
            Log(L"apply failed: 0x%08X %s", static_cast<unsigned>(error.code()), error.message().c_str());
        } catch (...) {
            Log(L"apply failed");
        }
    }
}

// Clocks that are on screen right now.
unsigned LiveClocks() {
    unsigned live = 0;
    for (auto& clock : g_clocks) {
        try {
            if (clock.time && clock.time.IsLoaded()) live++;
        } catch (...) {
        }
    }
    return live;
}

void Register(TextBlock const& time) {
    auto stack = VisualTreeHelper::GetParent(time).try_as<FrameworkElement>();
    if (!stack) return;

    Clock* clock = nullptr;
    for (auto& existing : g_clocks) {
        if (existing.stack == stack) clock = &existing;
    }
    if (!clock) {
        g_clocks.emplace_back();
        clock = &g_clocks.back();
        clock->stack = stack;
        Log(L"found a taskbar clock (%zu known)", g_clocks.size());
    }
    clock->time = time;
    if (g_active) Apply(*clock);
}

void Revert() {
    g_active = false;
    ApplyAll();
    if (g_host) KillTimer(g_host, OWNER_TIMER);
    if (g_owner) {
        CloseHandle(g_owner);
        g_owner = nullptr;
        g_ownerId = 0;
    }
}

// Duri may crash or be killed while its text is up; the Windows clock must
// come back on its own then.
void WatchOwner(DWORD processId) {
    if (processId != g_ownerId) {
        if (g_owner) CloseHandle(g_owner);
        g_owner = processId ? OpenProcess(SYNCHRONIZE, FALSE, processId) : nullptr;
        g_ownerId = processId;
    }
    SetTimer(g_host, OWNER_TIMER, OWNER_CHECK_MS, nullptr);
}

// ---------------------------------------------------------------------------
// XAML Diagnostics "TAP": XAML creates this object and hands it the tree.

struct Tap final : IObjectWithSite, IVisualTreeServiceCallback2 {
    std::atomic<ULONG> refs{1};
    winrt::com_ptr<IXamlDiagnostics> diagnostics;
    winrt::com_ptr<IVisualTreeService3> tree;

    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void** out) override {
        if (!out) return E_POINTER;
        if (iid == IID_IUnknown || iid == IID_IObjectWithSite) {
            *out = static_cast<IObjectWithSite*>(this);
        } else if (iid == __uuidof(IVisualTreeServiceCallback) || iid == __uuidof(IVisualTreeServiceCallback2)) {
            *out = static_cast<IVisualTreeServiceCallback2*>(this);
        } else {
            *out = nullptr;
            return E_NOINTERFACE;
        }
        AddRef();
        return S_OK;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return ++refs; }
    ULONG STDMETHODCALLTYPE Release() override {
        ULONG left = --refs;
        if (left == 0) delete this;
        return left;
    }

    HRESULT STDMETHODCALLTYPE SetSite(IUnknown* site) override {
        diagnostics = nullptr;
        tree = nullptr;
        if (!site) return S_OK;
        site->QueryInterface(__uuidof(IXamlDiagnostics), diagnostics.put_void());
        site->QueryInterface(__uuidof(IVisualTreeService3), tree.put_void());
        if (!diagnostics || !tree) {
            Log(L"XAML Diagnostics is missing an interface we need");
            return E_NOINTERFACE;
        }

        // AdviseVisualTreeChange replays the whole tree and must not run on
        // the UI thread it reports from.
        AddRef();
        HANDLE thread = CreateThread(
            nullptr, 0,
            [](void* param) -> DWORD {
                auto self = static_cast<Tap*>(param);
                HRESULT result = self->tree->AdviseVisualTreeChange(self);
                if (FAILED(result)) Log(L"AdviseVisualTreeChange failed: 0x%08X", static_cast<unsigned>(result));
                self->Release();
                return 0;
            },
            this, 0, nullptr);
        if (thread) {
            CloseHandle(thread);
        } else {
            Release();
        }
        return S_OK;
    }

    HRESULT STDMETHODCALLTYPE GetSite(REFIID iid, void** out) override {
        if (!diagnostics) return E_FAIL;
        return diagnostics->QueryInterface(iid, out);
    }

    HRESULT STDMETHODCALLTYPE OnVisualTreeChange(ParentChildRelation, VisualElement element,
                                                 VisualMutationType mutation) override {
        try {
            if (mutation != Add || !element.Name) return S_OK;
            if (wcscmp(element.Name, L"TimeInnerTextBlock") != 0) return S_OK;

            winrt::com_ptr<::IInspectable> raw;
            if (FAILED(diagnostics->GetIInspectableFromHandle(element.Handle, raw.put())) || !raw) return S_OK;
            auto block = raw.try_as<TextBlock>();
            if (!block) return S_OK;
            // Deferred so the element is parented by the time we look at it.
            block.Dispatcher().RunAsync(CoreDispatcherPriority::Normal, [block]() {
                try {
                    Register(block);
                } catch (...) {
                    Log(L"could not register a clock");
                }
            });
        } catch (...) {
        }
        return S_OK;
    }

    HRESULT STDMETHODCALLTYPE OnElementStateChanged(InstanceHandle, VisualElementState, LPCWSTR) override {
        return S_OK;
    }

    virtual ~Tap() = default;
};

struct TapFactory final : IClassFactory {
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void** out) override {
        if (!out) return E_POINTER;
        if (iid == IID_IUnknown || iid == IID_IClassFactory) {
            *out = static_cast<IClassFactory*>(this);
            return S_OK;
        }
        *out = nullptr;
        return E_NOINTERFACE;
    }
    // Static lifetime.
    ULONG STDMETHODCALLTYPE AddRef() override { return 2; }
    ULONG STDMETHODCALLTYPE Release() override { return 1; }
    HRESULT STDMETHODCALLTYPE CreateInstance(IUnknown* outer, REFIID iid, void** out) override {
        if (outer) return CLASS_E_NOAGGREGATION;
        auto tap = new (std::nothrow) Tap();
        if (!tap) return E_OUTOFMEMORY;
        HRESULT result = tap->QueryInterface(iid, out);
        tap->Release();
        return result;
    }
    HRESULT STDMETHODCALLTYPE LockServer(BOOL) override { return S_OK; }
};

TapFactory g_factory;

// False while XAML is not up yet in this process: try again later.
bool ConnectDiagnostics() {
    HMODULE xaml = GetModuleHandleW(L"Windows.UI.Xaml.dll");
    if (!xaml) return false;
    using Initialize = HRESULT(WINAPI*)(LPCWSTR, DWORD, LPCWSTR, LPCWSTR, CLSID, LPCWSTR);
    auto initialize = reinterpret_cast<Initialize>(GetProcAddress(xaml, "InitializeXamlDiagnosticsEx"));
    if (!initialize) return false;
    wchar_t self[MAX_PATH];
    if (!GetModuleFileNameW(g_module, self, MAX_PATH)) return false;

    // A connection name can be taken by another tool; walk until one is free.
    HRESULT result = E_FAIL;
    for (int i = 1; i <= 1000; i++) {
        wchar_t name[64];
        swprintf_s(name, L"VisualDiagConnection%d", i);
        result = initialize(name, GetCurrentProcessId(), L"", self, CLSID_DuriClockTap, nullptr);
        if (result != HRESULT_FROM_WIN32(ERROR_NOT_FOUND)) break;
    }
    if (FAILED(result)) Log(L"InitializeXamlDiagnosticsEx failed: 0x%08X", static_cast<unsigned>(result));
    return SUCCEEDED(result);
}

void TryConnect(HWND window) {
    static int attempts = 0;
    attempts++;
    if (ConnectDiagnostics()) {
        KillTimer(window, CONNECT_TIMER);
        Log(L"connected to the taskbar's XAML (attempt %d)", attempts);
    } else if (attempts >= CONNECT_ATTEMPTS) {
        KillTimer(window, CONNECT_TIMER);
        Log(L"gave up connecting to the taskbar's XAML");
    } else if (attempts == 1) {
        SetTimer(window, CONNECT_TIMER, CONNECT_RETRY_MS, nullptr);
    }
}

// Commands return 1 + the number of clocks on screen, so Duri can tell
// "delivered, but this Windows build has no clock we recognize" (1) apart.
LRESULT HandleCommand(const COPYDATASTRUCT& data) {
    if (data.dwData == CMD_REVERT) {
        Revert();
        return 1 + LiveClocks();
    }
    if (data.dwData != CMD_SET_TEXT || !data.lpData) return 0;
    if (data.cbData < sizeof(SetHeader) + sizeof(wchar_t)) return 0;

    SetHeader header{};
    memcpy(&header, data.lpData, sizeof(header));
    auto chars = reinterpret_cast<const wchar_t*>(static_cast<const BYTE*>(data.lpData) + sizeof(header));
    size_t count = (data.cbData - sizeof(header)) / sizeof(wchar_t);
    g_text.assign(chars, wcsnlen(chars, count));
    g_align = header.align;
    g_generation = header.generation;
    g_active = true;
    WatchOwner(header.owner);
    ApplyAll();
    return 1 + LiveClocks();
}

LRESULT CALLBACK HostProc(HWND window, UINT message, WPARAM wparam, LPARAM lparam) {
    try {
        switch (message) {
            case WM_HOST_INIT:
                TryConnect(window);
                return 0;
            case WM_COPYDATA:
                if (auto data = reinterpret_cast<const COPYDATASTRUCT*>(lparam)) return HandleCommand(*data);
                return 0;
            case WM_TIMER:
                if (wparam == CONNECT_TIMER) {
                    TryConnect(window);
                    return 0;
                }
                if (wparam == OWNER_TIMER && g_active &&
                    (!g_owner || WaitForSingleObject(g_owner, 0) != WAIT_TIMEOUT)) {
                    Log(L"Duri is gone, bringing the Windows clock back");
                    Revert();
                }
                return 0;
        }
    } catch (...) {
        return 0;
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

void Attach() {
    static bool attached = false;
    if (attached) return;
    attached = true;

    // Stay loaded after Duri removes its hook.
    HMODULE pinned = nullptr;
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_PIN | GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                       reinterpret_cast<LPCWSTR>(&Attach), &pinned);

    // The class is named after this file, so Duri finds exactly the build it
    // shipped and never talks to a DLL left over from an older version.
    wchar_t path[MAX_PATH];
    if (!GetModuleFileNameW(g_module, path, MAX_PATH)) return;
    std::wstring name = path;
    if (auto slash = name.find_last_of(L"\\/"); slash != std::wstring::npos) name.erase(0, slash + 1);
    if (auto dot = name.find_last_of(L'.'); dot != std::wstring::npos) name.erase(dot);
    std::wstring className = HOST_CLASS_PREFIX + name;

    WNDCLASSW windowClass{};
    windowClass.lpfnWndProc = HostProc;
    windowClass.hInstance = g_module;
    windowClass.lpszClassName = className.c_str();
    RegisterClassW(&windowClass);
    g_host = CreateWindowExW(0, className.c_str(), L"", 0, 0, 0, 0, 0, HWND_MESSAGE, nullptr, g_module, nullptr);
    if (!g_host) {
        Log(L"could not create the command window: %lu", GetLastError());
        return;
    }
    Log(L"attached to explorer %lu as %s", GetCurrentProcessId(), name.c_str());
    // Not from inside the hook: COM refuses calls while a sent message is
    // being dispatched.
    PostMessageW(g_host, WM_HOST_INIT, 0, 0);
}

}  // namespace

extern "C" LRESULT CALLBACK DuriHookProc(int code, WPARAM wparam, LPARAM lparam) {
    if (code == HC_ACTION) {
        static const UINT attachMessage = RegisterWindowMessageW(ATTACH_MESSAGE);
        auto call = reinterpret_cast<CWPSTRUCT*>(lparam);
        if (call && call->message == attachMessage) {
            try {
                Attach();
            } catch (...) {
            }
        }
    }
    return CallNextHookEx(nullptr, code, wparam, lparam);
}

STDAPI DllGetClassObject(REFCLSID clsid, REFIID iid, void** out) {
    if (clsid != CLSID_DuriClockTap) return CLASS_E_CLASSNOTAVAILABLE;
    return g_factory.QueryInterface(iid, out);
}

STDAPI DllCanUnloadNow() {
    return S_FALSE;
}

BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, void*) {
    if (reason == DLL_PROCESS_ATTACH) {
        g_module = instance;
        DisableThreadLibraryCalls(instance);
    }
    return TRUE;
}
