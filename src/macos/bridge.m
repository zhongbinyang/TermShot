#import <AppKit/AppKit.h>
#import <Carbon/Carbon.h>
#import <CoreGraphics/CoreGraphics.h>
#import <Foundation/Foundation.h>
#import <QuartzCore/QuartzCore.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#include <dispatch/dispatch.h>
#include <stdint.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

// Shared modifier bit constants matching src/native.rs
static const uint32_t TS_MOD_ALT = 1;
static const uint32_t TS_MOD_CONTROL = 2;
static const uint32_t TS_MOD_SHIFT = 4;
static const uint32_t TS_MOD_WIN = 8; // Cmd on macOS

// Shared cursor kinds
enum {
    TS_CURSOR_ARROW = 0,
    TS_CURSOR_HAND = 1,
    TS_CURSOR_CROSS = 2,
    TS_CURSOR_SIZEALL = 3,
    TS_CURSOR_SIZENS = 4,
    TS_CURSOR_SIZEWE = 5,
    TS_CURSOR_SIZENWSE = 6,
    TS_CURSOR_SIZENESW = 7,
    TS_CURSOR_IBEAM = 8,
};

// Shared canvas window kinds
enum {
    TS_WIN_OVERLAY = 0,
    TS_WIN_FLOATING = 1,
    TS_WIN_POPUP = 2,
    TS_WIN_TOAST = 3,
    TS_WIN_CLICKTHROUGH = 4,
    TS_WIN_DIALOG = 5,
};

// ============================================================================
// Coordinate & Color Helpers
// ============================================================================

static CGFloat primary_screen_height(void) {
    NSArray<NSScreen *> *screens = [NSScreen screens];
    if (screens.count > 0) {
        return screens[0].frame.size.height;
    }
    return 1080.0;
}

float ts_backing_scale(void) {
    CGFloat max_scale = 1.0;
    for (NSScreen *s in [NSScreen screens]) {
        if (s.backingScaleFactor > max_scale) {
            max_scale = s.backingScaleFactor;
        }
    }
    return (float)(max_scale > 0.0 ? max_scale : 1.0);
}

static NSRect cg_px_to_ns_rect(int32_t x, int32_t y, int32_t w, int32_t h) {
    CGFloat s = (CGFloat)ts_backing_scale();
    CGFloat h0 = primary_screen_height();
    CGFloat cg_x = (CGFloat)x / s;
    CGFloat cg_y = (CGFloat)y / s;
    CGFloat cg_w = (CGFloat)w / s;
    CGFloat cg_h = (CGFloat)h / s;
    CGFloat ns_y = h0 - (cg_y + cg_h);
    return NSMakeRect(cg_x, ns_y, cg_w, cg_h);
}

static void ns_rect_to_cg_px(NSRect r, int32_t *ox, int32_t *oy, int32_t *ow, int32_t *oh) {
    CGFloat s = (CGFloat)ts_backing_scale();
    CGFloat h0 = primary_screen_height();
    CGFloat cg_x = r.origin.x;
    CGFloat cg_y = h0 - (r.origin.y + r.size.height);
    if (ox) *ox = (int32_t)llround(cg_x * s);
    if (oy) *oy = (int32_t)llround(cg_y * s);
    if (ow) *ow = (int32_t)llround(r.size.width * s);
    if (oh) *oh = (int32_t)llround(r.size.height * s);
}

static NSColor *ns_color_argb(uint32_t argb) {
    CGFloat a = ((argb >> 24) & 0xFF) / 255.0;
    CGFloat r = ((argb >> 16) & 0xFF) / 255.0;
    CGFloat g = ((argb >> 8) & 0xFF) / 255.0;
    CGFloat b = (argb & 0xFF) / 255.0;
    return [NSColor colorWithSRGBRed:r green:g blue:b alpha:a];
}

static void set_ctx_fill_argb(CGContextRef ctx, uint32_t argb) {
    CGFloat a = ((argb >> 24) & 0xFF) / 255.0;
    CGFloat r = ((argb >> 16) & 0xFF) / 255.0;
    CGFloat g = ((argb >> 8) & 0xFF) / 255.0;
    CGFloat b = (argb & 0xFF) / 255.0;
    CGContextSetRGBFillColor(ctx, r, g, b, a);
}

static void set_ctx_stroke_argb(CGContextRef ctx, uint32_t argb) {
    CGFloat a = ((argb >> 24) & 0xFF) / 255.0;
    CGFloat r = ((argb >> 16) & 0xFF) / 255.0;
    CGFloat g = ((argb >> 8) & 0xFF) / 255.0;
    CGFloat b = (argb & 0xFF) / 255.0;
    CGContextSetRGBStrokeColor(ctx, r, g, b, a);
}

static NSFont *make_ui_font(CGFloat px, BOOL bold) {
    NSFontWeight weight = bold ? NSFontWeightSemibold : NSFontWeightRegular;
    NSFont *font = [NSFont fontWithName:@"PingFangSC-Regular" size:px];
    if (bold) {
        NSFont *b = [NSFont fontWithName:@"PingFangSC-Semibold" size:px];
        if (b) font = b;
    }
    if (!font) {
        font = [NSFont systemFontOfSize:px weight:weight];
    }
    return font;
}

static NSFont *make_mono_font(CGFloat px, BOOL bold) {
    NSFontWeight weight = bold ? NSFontWeightBold : NSFontWeightRegular;
    return [NSFont monospacedSystemFontOfSize:px weight:weight];
}

// ============================================================================
// KeyCode & Modifier Mapping (macOS CGKeyCode <-> Win32 VK codes)
// ============================================================================

static uint32_t modifiers_from_ns(NSEventModifierFlags flags) {
    uint32_t m = 0;
    if (flags & NSEventModifierFlagOption) m |= TS_MOD_ALT;
    if (flags & NSEventModifierFlagControl) m |= TS_MOD_CONTROL;
    if (flags & NSEventModifierFlagShift) m |= TS_MOD_SHIFT;
    if (flags & NSEventModifierFlagCommand) m |= TS_MOD_WIN;
    return m;
}

uint32_t ts_modifiers(void) {
    return modifiers_from_ns([NSEvent modifierFlags]);
}

static uint32_t mac_keycode_to_vk(uint16_t kc) {
    switch (kc) {
        case kVK_ANSI_A: return 'A';
        case kVK_ANSI_B: return 'B';
        case kVK_ANSI_C: return 'C';
        case kVK_ANSI_D: return 'D';
        case kVK_ANSI_E: return 'E';
        case kVK_ANSI_F: return 'F';
        case kVK_ANSI_G: return 'G';
        case kVK_ANSI_H: return 'H';
        case kVK_ANSI_I: return 'I';
        case kVK_ANSI_J: return 'J';
        case kVK_ANSI_K: return 'K';
        case kVK_ANSI_L: return 'L';
        case kVK_ANSI_M: return 'M';
        case kVK_ANSI_N: return 'N';
        case kVK_ANSI_O: return 'O';
        case kVK_ANSI_P: return 'P';
        case kVK_ANSI_Q: return 'Q';
        case kVK_ANSI_R: return 'R';
        case kVK_ANSI_S: return 'S';
        case kVK_ANSI_T: return 'T';
        case kVK_ANSI_U: return 'U';
        case kVK_ANSI_V: return 'V';
        case kVK_ANSI_W: return 'W';
        case kVK_ANSI_X: return 'X';
        case kVK_ANSI_Y: return 'Y';
        case kVK_ANSI_Z: return 'Z';
        case kVK_ANSI_0: case kVK_ANSI_Keypad0: return '0';
        case kVK_ANSI_1: case kVK_ANSI_Keypad1: return '1';
        case kVK_ANSI_2: case kVK_ANSI_Keypad2: return '2';
        case kVK_ANSI_3: case kVK_ANSI_Keypad3: return '3';
        case kVK_ANSI_4: case kVK_ANSI_Keypad4: return '4';
        case kVK_ANSI_5: case kVK_ANSI_Keypad5: return '5';
        case kVK_ANSI_6: case kVK_ANSI_Keypad6: return '6';
        case kVK_ANSI_7: case kVK_ANSI_Keypad7: return '7';
        case kVK_ANSI_8: case kVK_ANSI_Keypad8: return '8';
        case kVK_ANSI_9: case kVK_ANSI_Keypad9: return '9';
        case kVK_Escape: return 0x1B;
        case kVK_Return: case kVK_ANSI_KeypadEnter: return 0x0D;
        case kVK_Tab: return 0x09;
        case kVK_Space: return 0x20;
        case kVK_PageUp: return 0x21;
        case kVK_PageDown: return 0x22;
        case kVK_LeftArrow: return 0x25;
        case kVK_UpArrow: return 0x26;
        case kVK_RightArrow: return 0x27;
        case kVK_DownArrow: return 0x28;
        case kVK_ANSI_Equal: case kVK_ANSI_KeypadPlus: return 0xBB; // VK_OEM_PLUS
        case kVK_ANSI_Minus: case kVK_ANSI_KeypadMinus: return 0xBD; // VK_OEM_MINUS
        case kVK_F1: return 0x70;
        case kVK_F2: return 0x71;
        case kVK_F3: return 0x72;
        case kVK_F4: return 0x73;
        case kVK_F5: return 0x74;
        case kVK_F6: return 0x75;
        case kVK_F7: return 0x76;
        case kVK_F8: return 0x77;
        case kVK_F9: return 0x78;
        case kVK_F10: return 0x79;
        case kVK_F11: return 0x7A;
        case kVK_F12: return 0x7B;
        default: return 0;
    }
}

static int32_t vk_to_mac_keycode(uint32_t vk) {
    switch (vk) {
        case 'A': return kVK_ANSI_A;
        case 'B': return kVK_ANSI_B;
        case 'C': return kVK_ANSI_C;
        case 'D': return kVK_ANSI_D;
        case 'E': return kVK_ANSI_E;
        case 'F': return kVK_ANSI_F;
        case 'G': return kVK_ANSI_G;
        case 'H': return kVK_ANSI_H;
        case 'I': return kVK_ANSI_I;
        case 'J': return kVK_ANSI_J;
        case 'K': return kVK_ANSI_K;
        case 'L': return kVK_ANSI_L;
        case 'M': return kVK_ANSI_M;
        case 'N': return kVK_ANSI_N;
        case 'O': return kVK_ANSI_O;
        case 'P': return kVK_ANSI_P;
        case 'Q': return kVK_ANSI_Q;
        case 'R': return kVK_ANSI_R;
        case 'S': return kVK_ANSI_S;
        case 'T': return kVK_ANSI_T;
        case 'U': return kVK_ANSI_U;
        case 'V': return kVK_ANSI_V;
        case 'W': return kVK_ANSI_W;
        case 'X': return kVK_ANSI_X;
        case 'Y': return kVK_ANSI_Y;
        case 'Z': return kVK_ANSI_Z;
        case '0': return kVK_ANSI_0;
        case '1': return kVK_ANSI_1;
        case '2': return kVK_ANSI_2;
        case '3': return kVK_ANSI_3;
        case '4': return kVK_ANSI_4;
        case '5': return kVK_ANSI_5;
        case '6': return kVK_ANSI_6;
        case '7': return kVK_ANSI_7;
        case '8': return kVK_ANSI_8;
        case '9': return kVK_ANSI_9;
        case 0x1B: return kVK_Escape;
        case 0x0D: return kVK_Return;
        case 0x09: return kVK_Tab;
        case 0x20: return kVK_Space;
        case 0x21: return kVK_PageUp;
        case 0x22: return kVK_PageDown;
        case 0x25: return kVK_LeftArrow;
        case 0x26: return kVK_UpArrow;
        case 0x27: return kVK_RightArrow;
        case 0x28: return kVK_DownArrow;
        case 0xBB: case 0x6B: return kVK_ANSI_Equal;
        case 0xBD: case 0x6D: return kVK_ANSI_Minus;
        case 0x70: return kVK_F1;
        case 0x71: return kVK_F2;
        case 0x72: return kVK_F3;
        case 0x73: return kVK_F4;
        case 0x74: return kVK_F5;
        case 0x75: return kVK_F6;
        case 0x76: return kVK_F7;
        case 0x77: return kVK_F8;
        case 0x78: return kVK_F9;
        case 0x79: return kVK_F10;
        case 0x7A: return kVK_F11;
        case 0x7B: return kVK_F12;
        default: return -1;
    }
}

bool ts_key_down_vk(int32_t vk) {
    if (vk == 0x10) { // VK_SHIFT
        return ([NSEvent modifierFlags] & NSEventModifierFlagShift) != 0;
    }
    if (vk == 0x11) { // VK_CONTROL
        return ([NSEvent modifierFlags] & NSEventModifierFlagControl) != 0;
    }
    if (vk == 0x12) { // VK_MENU / Alt / Option
        return ([NSEvent modifierFlags] & NSEventModifierFlagOption) != 0;
    }
    if (vk == 0x5B || vk == 0x5C) { // Cmd
        return ([NSEvent modifierFlags] & NSEventModifierFlagCommand) != 0;
    }
    int32_t kc = vk_to_mac_keycode((uint32_t)vk);
    if (kc < 0) return false;
    return CGEventSourceKeyState(kCGEventSourceStateCombinedSessionState, (CGKeyCode)kc);
}

// ============================================================================
// App Initialization, Menu & Event Loop
// ============================================================================

static BOOL g_app_inited = NO;

static void install_main_menu(void) {
    NSMenu *menubar = [[NSMenu alloc] init];
    NSMenuItem *appItem = [[NSMenuItem alloc] init];
    [menubar addItem:appItem];
    NSMenu *appMenu = [[NSMenu alloc] init];
    [appMenu addItemWithTitle:@"Quit TermShot" action:@selector(terminate:) keyEquivalent:@"q"];
    [appItem setSubmenu:appMenu];

    // Standard Edit menu so Cmd+C / Cmd+V / Cmd+X / Cmd+A / Cmd+Z work in text fields
    NSMenuItem *editItem = [[NSMenuItem alloc] initWithTitle:@"Edit" action:nil keyEquivalent:@""];
    [menubar addItem:editItem];
    NSMenu *editMenu = [[NSMenu alloc] initWithTitle:@"Edit"];
    [editMenu addItemWithTitle:@"Undo" action:@selector(undo:) keyEquivalent:@"z"];
    [editMenu addItemWithTitle:@"Redo" action:@selector(redo:) keyEquivalent:@"Z"];
    [editMenu addItem:[NSMenuItem separatorItem]];
    [editMenu addItemWithTitle:@"Cut" action:@selector(cut:) keyEquivalent:@"x"];
    [editMenu addItemWithTitle:@"Copy" action:@selector(copy:) keyEquivalent:@"c"];
    [editMenu addItemWithTitle:@"Paste" action:@selector(paste:) keyEquivalent:@"v"];
    [editMenu addItemWithTitle:@"Select All" action:@selector(selectAll:) keyEquivalent:@"a"];
    [editItem setSubmenu:editMenu];

    [NSApp setMainMenu:menubar];
}

void ts_app_init(void) {
    if (g_app_inited) return;
    g_app_inited = YES;
    @autoreleasepool {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
        if (@available(macOS 10.14, *)) {
            [NSApp setAppearance:[NSAppearance appearanceNamed:NSAppearanceNameDarkAqua]];
        }
        install_main_menu();
        [NSApp finishLaunching];
    }
}

void ts_app_run(void) {
    ts_app_init();
    @autoreleasepool {
        [NSApp run];
    }
}

void ts_app_quit(void) {
    @autoreleasepool {
        [NSApp stop:nil];
        // Post a dummy event so nextEventMatchingMask wakes immediately
        NSEvent *ev = [NSEvent otherEventWithType:NSEventTypeApplicationDefined
                                         location:NSZeroPoint
                                    modifierFlags:0
                                        timestamp:0
                                     windowNumber:0
                                          context:nil
                                          subtype:0
                                            data1:0
                                            data2:0];
        [NSApp postEvent:ev atStart:YES];
    }
}

void ts_pump_events(uint32_t timeout_ms) {
    ts_app_init();
    @autoreleasepool {
        NSDate *until = (timeout_ms == 0)
            ? [NSDate distantPast]
            : [NSDate dateWithTimeIntervalSinceNow:(double)timeout_ms / 1000.0];
        for (int iter = 0; iter < 64; iter++) {
            NSEvent *ev = [NSApp nextEventMatchingMask:NSEventMaskAny
                                             untilDate:(iter == 0 ? until : [NSDate distantPast])
                                                inMode:NSDefaultRunLoopMode
                                               dequeue:YES];
            if (!ev) break;
            [NSApp sendEvent:ev];
            [NSApp updateWindows];
        }
        [NSApp updateWindows];
    }
}

void ts_dispatch_main(void (*cb)(void *), void *user_data) {
    if (!cb) return;
    CFRunLoopRef rl = CFRunLoopGetMain();
    if (!rl) return;
    CFRunLoopPerformBlock(rl, kCFRunLoopCommonModes, ^{
        @autoreleasepool {
            cb(user_data);
            [NSApp updateWindows];
        }
    });
    CFRunLoopWakeUp(rl);
}

void ts_sleep_ms(uint32_t ms) {
    [NSThread sleepForTimeInterval:(double)ms / 1000.0];
}

// ============================================================================
// Tray / Menu Bar Status Item
// ============================================================================

@interface TSTrayController : NSObject <NSMenuDelegate>
@property (nonatomic, strong) NSStatusItem *statusItem;
@property (nonatomic, strong) NSMenu *contextMenu;
@property (nonatomic, strong) NSMenuItem *closePinsItem;
@property (nonatomic, assign) void (*onLeftClick)(void *);
@property (nonatomic, assign) void (*onMenuAction)(int32_t, void *);
@property (nonatomic, assign) void *userData;
@property (nonatomic, assign) int32_t pinCount;
@end

@implementation TSTrayController
- (void)statusBarClicked:(id)sender {
    NSEvent *ev = [NSApp currentEvent];
    BOOL isRight = (ev.type == NSEventTypeRightMouseUp || ev.type == NSEventTypeRightMouseDown ||
                    ((ev.modifierFlags & NSEventModifierFlagControl) != 0));
    if (isRight) {
        [self.statusItem popUpStatusItemMenu:self.contextMenu];
        return;
    }
    if (self.onLeftClick) {
        self.onLeftClick(self.userData);
    }
}

- (void)menuWillOpen:(NSMenu *)menu {
    if (self.closePinsItem) {
        self.closePinsItem.enabled = (self.pinCount > 0);
    }
}

- (void)menuItemSelected:(NSMenuItem *)item {
    if (self.onMenuAction) {
        self.onMenuAction((int32_t)item.tag, self.userData);
    }
}
@end

static TSTrayController *g_tray = nil;

static NSImage *nsimage_from_bgra(const uint32_t *bgra, int32_t w, int32_t h, NSSize ptSize) {
    if (!bgra || w <= 0 || h <= 0) return nil;
    CGColorSpaceRef cs = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    CGContextRef ctx = CGBitmapContextCreate(
        (void *)bgra, (size_t)w, (size_t)h, 8, (size_t)w * 4, cs,
        kCGBitmapByteOrder32Little | kCGImageAlphaPremultipliedFirst);
    CGImageRef cg = ctx ? CGBitmapContextCreateImage(ctx) : NULL;
    if (ctx) CGContextRelease(ctx);
    CGColorSpaceRelease(cs);
    if (!cg) return nil;
    NSImage *img = [[NSImage alloc] initWithCGImage:cg size:ptSize];
    CGImageRelease(cg);
    return img;
}

void ts_tray_init(const uint32_t *bgra, int32_t w, int32_t h, const char *tooltip,
                  void (*on_left_click)(void *), void (*on_menu_action)(int32_t, void *),
                  void *user_data) {
    ts_app_init();
    @autoreleasepool {
        TSTrayController *ctrl = [[TSTrayController alloc] init];
        ctrl.onLeftClick = on_left_click;
        ctrl.onMenuAction = on_menu_action;
        ctrl.userData = user_data;
        ctrl.pinCount = 0;

        NSStatusItem *item = [[NSStatusBar systemStatusBar] statusItemWithLength:NSSquareStatusItemLength];
        NSImage *img = nsimage_from_bgra(bgra, w, h, NSMakeSize(18, 18));
        if (img) {
            [img setTemplate:NO];
            item.button.image = img;
        } else {
            item.button.title = @"TS";
        }
        if (tooltip) {
            item.button.toolTip = [NSString stringWithUTF8String:tooltip];
        }
        item.button.target = ctrl;
        item.button.action = @selector(statusBarClicked:);
        [item.button sendActionOn:(NSEventMaskLeftMouseUp | NSEventMaskRightMouseUp)];

        NSMenu *menu = [[NSMenu alloc] initWithTitle:@"TermShot"];
        menu.autoenablesItems = NO;
        menu.delegate = ctrl;

        NSArray<NSArray *> *entries = @[
            @[@"截图", @1],
            @[@"翻译剪贴板", @2],
            @[@"关闭全部贴图", @3],
            @[@"-", @0],
            @[@"打开保存目录", @4],
            @[@"设置", @5],
            @[@"-", @0],
            @[@"退出", @6],
        ];
        for (NSArray *pair in entries) {
            NSString *title = pair[0];
            NSInteger tag = [pair[1] integerValue];
            if ([title isEqualToString:@"-"]) {
                [menu addItem:[NSMenuItem separatorItem]];
            } else {
                NSMenuItem *mi = [[NSMenuItem alloc] initWithTitle:title
                                                            action:@selector(menuItemSelected:)
                                                     keyEquivalent:@""];
                mi.target = ctrl;
                mi.tag = tag;
                if (tag == 3) {
                    mi.enabled = NO;
                    ctrl.closePinsItem = mi;
                }
                [menu addItem:mi];
            }
        }
        ctrl.contextMenu = menu;
        ctrl.statusItem = item;
        g_tray = ctrl;
    }
}

void ts_tray_set_tooltip(const char *tooltip) {
    if (!g_tray || !tooltip) return;
    @autoreleasepool {
        g_tray.statusItem.button.toolTip = [NSString stringWithUTF8String:tooltip];
    }
}

void ts_tray_set_pin_count(int32_t pin_count) {
    if (!g_tray) return;
    g_tray.pinCount = pin_count;
}

bool ts_tray_anchor(int32_t *out_x, int32_t *out_y) {
    if (!g_tray || !g_tray.statusItem.button.window) return false;
    @autoreleasepool {
        NSRect wf = g_tray.statusItem.button.window.frame;
        CGFloat s = (CGFloat)ts_backing_scale();
        CGFloat h0 = primary_screen_height();
        CGFloat cg_x = wf.origin.x + wf.size.width * 0.5;
        CGFloat cg_y = h0 - wf.origin.y;
        if (out_x) *out_x = (int32_t)llround(cg_x * s);
        if (out_y) *out_y = (int32_t)llround(cg_y * s);
        return true;
    }
}

// ============================================================================
// Display, Cursor, Focus & Permissions
// ============================================================================

void ts_virtual_screen(int32_t *ox, int32_t *oy, int32_t *ow, int32_t *oh) {
    @autoreleasepool {
        NSRect uni = NSZeroRect;
        for (NSScreen *sc in [NSScreen screens]) {
            uni = NSUnionRect(uni, sc.frame);
        }
        if (NSIsEmptyRect(uni)) {
            uni = NSMakeRect(0, 0, 1440, 900);
        }
        ns_rect_to_cg_px(uni, ox, oy, ow, oh);
    }
}

void ts_monitor_at(int32_t px, int32_t py, bool work_area,
                   int32_t *ox, int32_t *oy, int32_t *ow, int32_t *oh) {
    @autoreleasepool {
        CGFloat s = (CGFloat)ts_backing_scale();
        CGFloat h0 = primary_screen_height();
        NSPoint pt = NSMakePoint((CGFloat)px / s, h0 - (CGFloat)py / s);
        NSScreen *found = nil;
        CGFloat bestDist = 1e18;
        for (NSScreen *sc in [NSScreen screens]) {
            NSRect f = sc.frame;
            if (NSPointInRect(pt, f)) {
                found = sc;
                break;
            }
            CGFloat cx = NSMidX(f), cy = NSMidY(f);
            CGFloat d = (pt.x - cx) * (pt.x - cx) + (pt.y - cy) * (pt.y - cy);
            if (d < bestDist) {
                bestDist = d;
                found = sc;
            }
        }
        if (!found) found = [NSScreen mainScreen];
        if (!found) {
            ts_virtual_screen(ox, oy, ow, oh);
            return;
        }
        NSRect r = work_area ? found.visibleFrame : found.frame;
        ns_rect_to_cg_px(r, ox, oy, ow, oh);
    }
}

void ts_cursor_pos(int32_t *ox, int32_t *oy) {
    @autoreleasepool {
        NSPoint loc = [NSEvent mouseLocation];
        CGFloat s = (CGFloat)ts_backing_scale();
        CGFloat h0 = primary_screen_height();
        if (ox) *ox = (int32_t)llround(loc.x * s);
        if (oy) *oy = (int32_t)llround((h0 - loc.y) * s);
    }
}

void ts_set_cursor(int32_t kind) {
    @autoreleasepool {
        switch (kind) {
            case TS_CURSOR_HAND: [[NSCursor pointingHandCursor] set]; break;
            case TS_CURSOR_CROSS: [[NSCursor crosshairCursor] set]; break;
            case TS_CURSOR_SIZEALL: [[NSCursor openHandCursor] set]; break;
            case TS_CURSOR_SIZENS: [[NSCursor resizeUpDownCursor] set]; break;
            case TS_CURSOR_SIZEWE: [[NSCursor resizeLeftRightCursor] set]; break;
            case TS_CURSOR_SIZENWSE:
            case TS_CURSOR_SIZENESW:
                [[NSCursor crosshairCursor] set];
                break;
            case TS_CURSOR_IBEAM: [[NSCursor IBeamCursor] set]; break;
            default: [[NSCursor arrowCursor] set]; break;
        }
    }
}

int32_t ts_frontmost_pid(void) {
    @autoreleasepool {
        NSRunningApplication *app = [[NSWorkspace sharedWorkspace] frontmostApplication];
        return app ? (int32_t)app.processIdentifier : 0;
    }
}

void ts_activate_pid(int32_t pid) {
    if (pid <= 0) return;
    @autoreleasepool {
        NSRunningApplication *app = [NSRunningApplication runningApplicationWithProcessIdentifier:(pid_t)pid];
        if (app) {
            if (@available(macOS 14.0, *)) {
                [app activateWithOptions:0];
            } else {
                [app activateWithOptions:NSApplicationActivateIgnoringOtherApps];
            }
        }
    }
}

bool ts_has_screen_permission(void) {
    return CGPreflightScreenCaptureAccess();
}

bool ts_request_screen_permission(void) {
    return CGRequestScreenCaptureAccess();
}

void ts_open_screen_permission_settings(void) {
    @autoreleasepool {
        NSURL *url = [NSURL URLWithString:@"x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"];
        [[NSWorkspace sharedWorkspace] openURL:url];
    }
}

// ============================================================================
// Clipboard (NSPasteboard)
// ============================================================================

char *ts_clipboard_get_text(void) {
    @autoreleasepool {
        NSPasteboard *pb = [NSPasteboard generalPasteboard];
        NSString *s = [pb stringForType:NSPasteboardTypeString];
        if (!s) return NULL;
        const char *utf8 = [s UTF8String];
        if (!utf8) return NULL;
        return strdup(utf8);
    }
}

void ts_free_string(char *s) {
    if (s) free(s);
}

bool ts_clipboard_set_text(const char *text) {
    if (!text) return false;
    @autoreleasepool {
        NSString *s = [NSString stringWithUTF8String:text];
        if (!s) return false;
        NSPasteboard *pb = [NSPasteboard generalPasteboard];
        [pb clearContents];
        return [pb setString:s forType:NSPasteboardTypeString];
    }
}

bool ts_clipboard_set_png(const uint8_t *png_bytes, size_t len) {
    if (!png_bytes || len == 0) return false;
    @autoreleasepool {
        NSData *data = [NSData dataWithBytes:png_bytes length:len];
        NSImage *img = [[NSImage alloc] initWithData:data];
        if (!img) return false;
        NSPasteboard *pb = [NSPasteboard generalPasteboard];
        [pb clearContents];
        [pb setData:data forType:NSPasteboardTypePNG];
        return [pb writeObjects:@[img]];
    }
}

// ============================================================================
// Window Enumeration & Screen Capture
// ============================================================================

typedef struct {
    uint32_t window_id;
    int32_t x;
    int32_t y;
    int32_t w;
    int32_t h;
    char *title;
} TSWindowInfo;

int32_t ts_enum_windows(TSWindowInfo **out_list) {
    if (!out_list) return 0;
    *out_list = NULL;
    @autoreleasepool {
        CFArrayRef arr = CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID);
        if (!arr) return 0;

        NSArray *windows = (__bridge NSArray *)arr;
        pid_t selfPid = [[NSProcessInfo processInfo] processIdentifier];
        CGFloat s = (CGFloat)ts_backing_scale();

        TSWindowInfo *list = (TSWindowInfo *)calloc(windows.count, sizeof(TSWindowInfo));
        int32_t count = 0;

        for (NSDictionary *dict in windows) {
            NSNumber *layerNum = dict[(id)kCGWindowLayer];
            if (!layerNum || layerNum.intValue != 0) continue;

            NSNumber *alphaNum = dict[(id)kCGWindowAlpha];
            if (alphaNum && alphaNum.doubleValue < 0.05) continue;

            NSNumber *pidNum = dict[(id)kCGWindowOwnerPID];
            if (pidNum && pidNum.intValue == selfPid) continue;

            NSDictionary *boundsDict = dict[(id)kCGWindowBounds];
            CGRect bounds = CGRectZero;
            if (!boundsDict || !CGRectMakeWithDictionaryRepresentation((__bridge CFDictionaryRef)boundsDict, &bounds)) {
                continue;
            }
            if (bounds.size.width < 8.0 || bounds.size.height < 8.0) continue;

            NSString *owner = dict[(id)kCGWindowOwnerName] ?: @"";
            if ([owner isEqualToString:@"Dock"] || [owner isEqualToString:@"Window Server"]) {
                continue;
            }
            NSString *name = dict[(id)kCGWindowName] ?: owner;
            NSNumber *widNum = dict[(id)kCGWindowNumber];

            TSWindowInfo *item = &list[count++];
            item->window_id = widNum ? widNum.unsignedIntValue : 0;
            item->x = (int32_t)llround(bounds.origin.x * s);
            item->y = (int32_t)llround(bounds.origin.y * s);
            item->w = (int32_t)llround(bounds.size.width * s);
            item->h = (int32_t)llround(bounds.size.height * s);
            const char *utf8 = [name UTF8String];
            item->title = utf8 ? strdup(utf8) : strdup("");
        }
        CFRelease(arr);
        *out_list = list;
        return count;
    }
}

void ts_free_window_list(TSWindowInfo *list, int32_t count) {
    if (!list) return;
    for (int32_t i = 0; i < count; i++) {
        if (list[i].title) free(list[i].title);
    }
    free(list);
}

static void wait_with_runloop(dispatch_semaphore_t sem, double timeout_sec) {
    NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:timeout_sec];
    while (dispatch_semaphore_wait(sem, dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_MSEC)) != 0) {
        if ([[NSDate date] compare:deadline] == NSOrderedDescending) {
            break;
        }
        if ([NSThread isMainThread]) {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.002, true);
        }
    }
}

static int32_t render_cgimage_to_bgra(CGImageRef img, int32_t w, int32_t h, uint32_t *out_bgra) {
    if (!img || w <= 0 || h <= 0 || !out_bgra) return -1;
    CGColorSpaceRef cs = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    CGContextRef ctx = CGBitmapContextCreate(
        out_bgra, (size_t)w, (size_t)h, 8, (size_t)w * 4, cs,
        kCGBitmapByteOrder32Little | kCGImageAlphaPremultipliedFirst);
    CGColorSpaceRelease(cs);
    if (!ctx) return -1;
    CGContextSetInterpolationQuality(ctx, kCGInterpolationHigh);
    CGContextDrawImage(ctx, CGRectMake(0, 0, (CGFloat)w, (CGFloat)h), img);
    CGContextRelease(ctx);
    return 0;
}

int32_t ts_capture_rect(int32_t x, int32_t y, int32_t w, int32_t h, uint32_t *out_bgra) {
    if (w <= 0 || h <= 0 || !out_bgra) return -1;
    if (!CGPreflightScreenCaptureAccess()) {
        CGRequestScreenCaptureAccess();
        if (!CGPreflightScreenCaptureAccess()) {
            return -2;
        }
    }
    @autoreleasepool {
        CGFloat s = (CGFloat)ts_backing_scale();
        CGRect cgRect = CGRectMake((CGFloat)x / s, (CGFloat)y / s, (CGFloat)w / s, (CGFloat)h / s);

        if (@available(macOS 15.2, *)) {
            __block CGImageRef captured = NULL;
            __block BOOL hasErr = NO;
            dispatch_semaphore_t sem = dispatch_semaphore_create(0);
            [SCScreenshotManager captureImageInRect:cgRect
                                  completionHandler:^(CGImageRef _Nullable image, NSError * _Nullable error) {
                if (image) captured = CGImageRetain(image);
                if (error) hasErr = YES;
                dispatch_semaphore_signal(sem);
            }];
            wait_with_runloop(sem, 5.0);
            if (captured) {
                int32_t rc = render_cgimage_to_bgra(captured, w, h, out_bgra);
                CGImageRelease(captured);
                return rc;
            }
            if (hasErr && !CGPreflightScreenCaptureAccess()) {
                return -2;
            }
        }

        if (@available(macOS 14.0, *)) {
            __block SCShareableContent *content = nil;
            dispatch_semaphore_t csem = dispatch_semaphore_create(0);
            [SCShareableContent getShareableContentExcludingDesktopWindows:NO
                                                      onScreenWindowsOnly:YES
                                                        completionHandler:^(SCShareableContent * _Nullable sc, NSError * _Nullable error) {
                content = sc;
                dispatch_semaphore_signal(csem);
            }];
            wait_with_runloop(csem, 5.0);
            if (!content || content.displays.count == 0) {
                return -2;
            }

            CGColorSpaceRef cs = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
            CGContextRef ctx = CGBitmapContextCreate(
                out_bgra, (size_t)w, (size_t)h, 8, (size_t)w * 4, cs,
                kCGBitmapByteOrder32Little | kCGImageAlphaPremultipliedFirst);
            CGColorSpaceRelease(cs);
            if (!ctx) return -1;
            CGContextSetRGBFillColor(ctx, 0, 0, 0, 1);
            CGContextFillRect(ctx, CGRectMake(0, 0, w, h));

            BOOL drewAny = NO;
            for (SCDisplay *disp in content.displays) {
                CGRect df = disp.frame;
                CGRect inter = CGRectIntersection(cgRect, df);
                if (CGRectIsEmpty(inter) || inter.size.width < 1.0 || inter.size.height < 1.0) continue;

                SCContentFilter *filter = [[SCContentFilter alloc] initWithDisplay:disp excludingWindows:@[]];
                SCStreamConfiguration *cfg = [[SCStreamConfiguration alloc] init];
                cfg.sourceRect = CGRectMake(inter.origin.x - df.origin.x,
                                            inter.origin.y - df.origin.y,
                                            inter.size.width,
                                            inter.size.height);
                cfg.width = (size_t)llround(inter.size.width * s);
                cfg.height = (size_t)llround(inter.size.height * s);
                cfg.showsCursor = NO;
                cfg.pixelFormat = 'BGRA';

                __block CGImageRef part = NULL;
                dispatch_semaphore_t psem = dispatch_semaphore_create(0);
                [SCScreenshotManager captureImageWithFilter:filter
                                              configuration:cfg
                                          completionHandler:^(CGImageRef _Nullable image, NSError * _Nullable error) {
                    if (image) part = CGImageRetain(image);
                    dispatch_semaphore_signal(psem);
                }];
                wait_with_runloop(psem, 5.0);
                if (part) {
                    // CGBitmapContext origin is bottom-left; convert CG inter rect into target pixel rect
                    CGFloat dx = (inter.origin.x - cgRect.origin.x) * s;
                    CGFloat dy_top = (inter.origin.y - cgRect.origin.y) * s;
                    CGFloat dw = inter.size.width * s;
                    CGFloat dh = inter.size.height * s;
                    CGFloat dy_bot = (CGFloat)h - (dy_top + dh);
                    CGContextDrawImage(ctx, CGRectMake(dx, dy_bot, dw, dh), part);
                    CGImageRelease(part);
                    drewAny = YES;
                }
            }
            CGContextRelease(ctx);
            return drewAny ? 0 : -1;
        }
        return -1;
    }
}

// ============================================================================
// Global Hotkeys (Carbon RegisterEventHotKey)
// ============================================================================

#define MAX_HOTKEYS 16
typedef struct {
    int32_t id;
    EventHotKeyRef ref;
    void (*cb)(int32_t, void *);
    void *user_data;
    bool active;
} TSHotkeySlot;

static TSHotkeySlot g_hotkeys[MAX_HOTKEYS];
static EventHandlerRef g_hotkey_handler_ref = NULL;

static OSStatus ts_carbon_hotkey_handler(EventHandlerCallRef next, EventRef event, void *ud) {
    EventHotKeyID hkId;
    OSStatus err = GetEventParameter(event, kEventParamDirectObject, typeEventHotKeyID,
                                     NULL, sizeof(hkId), NULL, &hkId);
    if (err != noErr || hkId.signature != 'TSHT') return eventNotHandledErr;
    for (int i = 0; i < MAX_HOTKEYS; i++) {
        if (g_hotkeys[i].active && g_hotkeys[i].id == (int32_t)hkId.id) {
            if (g_hotkeys[i].cb) {
                g_hotkeys[i].cb(g_hotkeys[i].id, g_hotkeys[i].user_data);
            }
            return noErr;
        }
    }
    return eventNotHandledErr;
}

bool ts_register_hotkey(int32_t hotkey_id, uint32_t mods, uint32_t vk,
                        void (*cb)(int32_t, void *), void *user_data) {
    int32_t kc = vk_to_mac_keycode(vk);
    if (kc < 0) return false;

    if (!g_hotkey_handler_ref) {
        EventTypeSpec spec = { kEventClassKeyboard, kEventHotKeyPressed };
        InstallApplicationEventHandler(NewEventHandlerUPP(ts_carbon_hotkey_handler),
                                       1, &spec, NULL, &g_hotkey_handler_ref);
    }

    uint32_t cmods = 0;
    if (mods & TS_MOD_ALT) cmods |= optionKey;
    if (mods & TS_MOD_CONTROL) cmods |= controlKey;
    if (mods & TS_MOD_SHIFT) cmods |= shiftKey;
    if (mods & TS_MOD_WIN) cmods |= cmdKey;

    int freeIdx = -1;
    for (int i = 0; i < MAX_HOTKEYS; i++) {
        if (g_hotkeys[i].active && g_hotkeys[i].id == hotkey_id) {
            if (g_hotkeys[i].ref) UnregisterEventHotKey(g_hotkeys[i].ref);
            g_hotkeys[i].active = false;
            freeIdx = i;
            break;
        }
        if (!g_hotkeys[i].active && freeIdx < 0) {
            freeIdx = i;
        }
    }
    if (freeIdx < 0) return false;

    EventHotKeyID hkId = { 'TSHT', (UInt32)hotkey_id };
    EventHotKeyRef ref = NULL;
    OSStatus st = RegisterEventHotKey((UInt32)kc, cmods, hkId, GetApplicationEventTarget(), 0, &ref);
    if (st != noErr || !ref) return false;

    g_hotkeys[freeIdx].id = hotkey_id;
    g_hotkeys[freeIdx].ref = ref;
    g_hotkeys[freeIdx].cb = cb;
    g_hotkeys[freeIdx].user_data = user_data;
    g_hotkeys[freeIdx].active = true;
    return true;
}

void ts_unregister_hotkey(int32_t hotkey_id) {
    for (int i = 0; i < MAX_HOTKEYS; i++) {
        if (g_hotkeys[i].active && g_hotkeys[i].id == hotkey_id) {
            if (g_hotkeys[i].ref) UnregisterEventHotKey(g_hotkeys[i].ref);
            g_hotkeys[i].ref = NULL;
            g_hotkeys[i].active = false;
        }
    }
}

// ============================================================================
// Text Measurement & Bitmap Text Rendering
// ============================================================================

void ts_measure_text(const char *text, int32_t px, bool bold, int32_t *out_w, int32_t *out_h) {
    if (!text || !*text) {
        if (out_w) *out_w = 0;
        if (out_h) *out_h = px;
        return;
    }
    @autoreleasepool {
        NSString *str = [NSString stringWithUTF8String:text] ?: @"";
        NSFont *font = make_ui_font((CGFloat)(px > 0 ? px : 12), bold);
        NSSize sz = [str sizeWithAttributes:@{ NSFontAttributeName: font }];
        if (out_w) *out_w = (int32_t)ceil(sz.width);
        if (out_h) *out_h = (int32_t)ceil(sz.height);
    }
}

int32_t ts_measure_wrapped_text(const char *text, int32_t max_w, int32_t px, bool bold) {
    if (!text || !*text) return 0;
    @autoreleasepool {
        NSString *str = [NSString stringWithUTF8String:text] ?: @"";
        NSFont *font = make_ui_font((CGFloat)(px > 0 ? px : 12), bold);
        NSRect r = [str boundingRectWithSize:NSMakeSize((CGFloat)(max_w > 0 ? max_w : 100), 10000.0)
                                     options:(NSStringDrawingUsesLineFragmentOrigin | NSStringDrawingUsesFontLeading)
                                  attributes:@{ NSFontAttributeName: font }];
        return (int32_t)ceil(r.size.height);
    }
}

void ts_draw_text_bitmap(uint32_t *bgra, int32_t bw, int32_t bh,
                         int32_t x, int32_t y, const char *text,
                         uint32_t argb, int32_t px, bool bold) {
    if (!bgra || bw <= 0 || bh <= 0 || !text || !*text) return;
    @autoreleasepool {
        NSString *str = [NSString stringWithUTF8String:text];
        if (!str || str.length == 0) return;
        NSFont *font = make_ui_font((CGFloat)(px > 0 ? px : 12), bold);
        NSDictionary *attrs = @{
            NSFontAttributeName: font,
            NSForegroundColorAttributeName: [NSColor whiteColor],
        };
        NSSize sz = [str sizeWithAttributes:attrs];
        int32_t tw = ((int32_t)ceil(sz.width) + 4);
        int32_t th = ((int32_t)ceil(sz.height) + 4);
        if (tw <= 0 || th <= 0) return;

        uint32_t *tmp = (uint32_t *)calloc((size_t)tw * (size_t)th, sizeof(uint32_t));
        if (!tmp) return;

        CGColorSpaceRef cs = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
        CGContextRef ctx = CGBitmapContextCreate(
            tmp, (size_t)tw, (size_t)th, 8, (size_t)tw * 4, cs,
            kCGBitmapByteOrder32Little | kCGImageAlphaPremultipliedFirst);
        CGColorSpaceRelease(cs);
        if (!ctx) {
            free(tmp);
            return;
        }

        [NSGraphicsContext saveGraphicsState];
        NSGraphicsContext *nsCtx = [NSGraphicsContext graphicsContextWithCGContext:ctx flipped:YES];
        [NSGraphicsContext setCurrentContext:nsCtx];
        CGContextTranslateCTM(ctx, 0, (CGFloat)th);
        CGContextScaleCTM(ctx, 1.0, -1.0);
        [str drawAtPoint:NSMakePoint(0, 0) withAttributes:attrs];
        [NSGraphicsContext restoreGraphicsState];
        CGContextRelease(ctx);

        uint32_t cr = (argb >> 16) & 0xFF;
        uint32_t cg = (argb >> 8) & 0xFF;
        uint32_t cb = argb & 0xFF;
        uint32_t ca = (argb >> 24) & 0xFF;
        if (ca == 0) ca = 255;

        for (int32_t py = 0; py < th; py++) {
            int32_t dy = y + py;
            if (dy < 0 || dy >= bh) continue;
            for (int32_t px_i = 0; px_i < tw; px_i++) {
                int32_t dx = x + px_i;
                if (dx < 0 || dx >= bw) continue;
                uint32_t a = (tmp[py * tw + px_i] >> 24) & 0xFF;
                if (a == 0) continue;
                uint32_t eff_a = (a * ca) / 255;
                if (eff_a == 0) continue;
                uint32_t dst = bgra[dy * bw + dx];
                uint32_t db = dst & 0xFF;
                uint32_t dg = (dst >> 8) & 0xFF;
                uint32_t dr = (dst >> 16) & 0xFF;
                uint32_t ia = 255 - eff_a;
                uint32_t out_r = (cr * eff_a + dr * ia) / 255;
                uint32_t out_g = (cg * eff_a + dg * ia) / 255;
                uint32_t out_b = (cb * eff_a + db * ia) / 255;
                bgra[dy * bw + dx] = (0xFFu << 24) | (out_r << 16) | (out_g << 8) | out_b;
            }
        }
        free(tmp);
    }
}

// ============================================================================
// Reusable Canvas Window & View (Overlay, CapturePin, Pin, QuickPanel, Toast, Scroll, Setup)
// ============================================================================

typedef struct {
    void (*on_paint)(void *ctx, int32_t w, int32_t h, void *user_data);
    void (*on_mouse_move)(int32_t x, int32_t y, void *user_data);
    void (*on_mouse_leave)(void *user_data);
    void (*on_mouse_down)(int32_t x, int32_t y, int32_t button, int32_t click_count, void *user_data);
    void (*on_mouse_up)(int32_t x, int32_t y, int32_t button, void *user_data);
    void (*on_scroll)(double delta_y, int32_t x, int32_t y, uint32_t mods, void *user_data);
    void (*on_key_down)(uint32_t vk, uint32_t mods, void *user_data);
    void (*on_resign_key)(void *user_data);
    void (*on_timer)(void *user_data);
    void (*on_close)(void *user_data);
    void (*on_text_commit)(const char *text, bool commit, void *user_data);
} TSCanvasCallbacks;

@class TSCanvasView;

@interface TSCanvasWindow : NSPanel <NSWindowDelegate>
@property (nonatomic, assign) BOOL allowKey;
@property (nonatomic, assign) TSCanvasCallbacks cbs;
@property (nonatomic, assign) void *userData;
@property (nonatomic, strong) TSCanvasView *canvasView;
@property (nonatomic, strong) NSTimer *timer;
@property (nonatomic, assign) BOOL closedNotified;
@end

@interface TSCanvasView : NSView <NSTextFieldDelegate>
@property (nonatomic, weak) TSCanvasWindow *owner;
@property (nonatomic, strong) NSTextField *inlineEdit;
@property (nonatomic, strong) NSTrackingArea *trackingArea;
@end

@implementation TSCanvasWindow
- (BOOL)canBecomeKeyWindow { return self.allowKey; }
- (BOOL)canBecomeMainWindow { return self.allowKey; }
- (NSRect)constrainFrameRect:(NSRect)frameRect toScreen:(NSScreen *)screen {
    return frameRect;
}

- (void)windowDidResignKey:(NSNotification *)notification {
    if (self.cbs.on_resign_key) {
        self.cbs.on_resign_key(self.userData);
    }
}

- (void)windowWillClose:(NSNotification *)notification {
    if (self.timer) {
        [self.timer invalidate];
        self.timer = nil;
    }
    if (!self.closedNotified && self.cbs.on_close) {
        self.closedNotified = YES;
        self.cbs.on_close(self.userData);
    }
}

- (void)timerFired:(NSTimer *)t {
    if (self.cbs.on_timer) {
        self.cbs.on_timer(self.userData);
    }
}
@end

@implementation TSCanvasView
- (BOOL)isFlipped { return YES; }
- (BOOL)acceptsFirstResponder { return YES; }
- (BOOL)acceptsFirstMouse:(NSEvent *)event { return YES; }

- (void)updateTrackingAreas {
    [super updateTrackingAreas];
    if (self.trackingArea) {
        [self removeTrackingArea:self.trackingArea];
    }
    NSTrackingAreaOptions opts = NSTrackingMouseMoved
                               | NSTrackingMouseEnteredAndExited
                               | NSTrackingActiveAlways
                               | NSTrackingInVisibleRect;
    self.trackingArea = [[NSTrackingArea alloc] initWithRect:self.bounds options:opts owner:self userInfo:nil];
    [self addTrackingArea:self.trackingArea];
}

- (void)drawRect:(NSRect)dirtyRect {
    TSCanvasWindow *w = self.owner;
    if (!w || !w.cbs.on_paint) return;
    CGContextRef ctx = [[NSGraphicsContext currentContext] CGContext];
    if (!ctx) return;
    CGFloat s = (CGFloat)ts_backing_scale();
    int32_t pw = (int32_t)llround(self.bounds.size.width * s);
    int32_t ph = (int32_t)llround(self.bounds.size.height * s);
    CGContextSaveGState(ctx);
    CGContextScaleCTM(ctx, 1.0 / s, 1.0 / s);
    w.cbs.on_paint((void *)ctx, pw, ph, w.userData);
    CGContextRestoreGState(ctx);
}

- (void)pointFromEvent:(NSEvent *)ev outX:(int32_t *)ox outY:(int32_t *)oy {
    NSPoint p = [self convertPoint:ev.locationInWindow fromView:nil];
    CGFloat s = (CGFloat)ts_backing_scale();
    *ox = (int32_t)llround(p.x * s);
    *oy = (int32_t)llround(p.y * s);
}

- (void)mouseMoved:(NSEvent *)event {
    if (!self.owner.cbs.on_mouse_move) return;
    int32_t x, y;
    [self pointFromEvent:event outX:&x outY:&y];
    self.owner.cbs.on_mouse_move(x, y, self.owner.userData);
}

- (void)mouseDragged:(NSEvent *)event {
    [self mouseMoved:event];
}

- (void)rightMouseDragged:(NSEvent *)event {
    [self mouseMoved:event];
}

- (void)mouseExited:(NSEvent *)event {
    if (self.owner.cbs.on_mouse_leave) {
        self.owner.cbs.on_mouse_leave(self.owner.userData);
    }
}

- (void)mouseDown:(NSEvent *)event {
    if (!self.owner.cbs.on_mouse_down) return;
    int32_t x, y;
    [self pointFromEvent:event outX:&x outY:&y];
    self.owner.cbs.on_mouse_down(x, y, 0, (int32_t)event.clickCount, self.owner.userData);
}

- (void)mouseUp:(NSEvent *)event {
    if (!self.owner.cbs.on_mouse_up) return;
    int32_t x, y;
    [self pointFromEvent:event outX:&x outY:&y];
    self.owner.cbs.on_mouse_up(x, y, 0, self.owner.userData);
}

- (void)rightMouseDown:(NSEvent *)event {
    if (!self.owner.cbs.on_mouse_down) return;
    int32_t x, y;
    [self pointFromEvent:event outX:&x outY:&y];
    self.owner.cbs.on_mouse_down(x, y, 1, (int32_t)event.clickCount, self.owner.userData);
}

- (void)rightMouseUp:(NSEvent *)event {
    if (!self.owner.cbs.on_mouse_up) return;
    int32_t x, y;
    [self pointFromEvent:event outX:&x outY:&y];
    self.owner.cbs.on_mouse_up(x, y, 1, self.owner.userData);
}

- (void)scrollWheel:(NSEvent *)event {
    if (!self.owner.cbs.on_scroll) return;
    int32_t x, y;
    [self pointFromEvent:event outX:&x outY:&y];
    double dy = event.scrollingDeltaY;
    if (dy == 0.0) dy = event.deltaY;
    self.owner.cbs.on_scroll(dy, x, y, modifiers_from_ns(event.modifierFlags), self.owner.userData);
}

- (void)keyDown:(NSEvent *)event {
    if (self.inlineEdit) {
        [super keyDown:event];
        return;
    }
    if (self.owner.cbs.on_key_down) {
        uint32_t vk = mac_keycode_to_vk(event.keyCode);
        if (vk != 0) {
            self.owner.cbs.on_key_down(vk, modifiers_from_ns(event.modifierFlags), self.owner.userData);
            return;
        }
    }
    [super keyDown:event];
}

- (BOOL)performKeyEquivalent:(NSEvent *)event {
    if (self.inlineEdit) {
        return [super performKeyEquivalent:event];
    }
    if (self.owner.cbs.on_key_down) {
        uint32_t vk = mac_keycode_to_vk(event.keyCode);
        if (vk == 0x1B || vk == 0x0D || vk == 0x09) {
            self.owner.cbs.on_key_down(vk, modifiers_from_ns(event.modifierFlags), self.owner.userData);
            return YES;
        }
    }
    return [super performKeyEquivalent:event];
}

- (BOOL)control:(NSControl *)control textView:(NSTextView *)textView doCommandBySelector:(SEL)commandSelector {
    if (commandSelector == @selector(insertNewline:)) {
        if (self.owner.cbs.on_text_commit) {
            NSString *val = self.inlineEdit ? (self.inlineEdit.stringValue ?: @"") : @"";
            const char *utf8 = [val UTF8String] ?: "";
            [self.inlineEdit removeFromSuperview];
            self.inlineEdit = nil;
            [self.owner makeFirstResponder:self];
            self.owner.cbs.on_text_commit(utf8, true, self.owner.userData);
        }
        return YES;
    }
    if (commandSelector == @selector(cancelOperation:)) {
        if (self.owner.cbs.on_text_commit) {
            [self.inlineEdit removeFromSuperview];
            self.inlineEdit = nil;
            [self.owner makeFirstResponder:self];
            self.owner.cbs.on_text_commit("", false, self.owner.userData);
        }
        return YES;
    }
    return NO;
}
@end

void *ts_canvas_create(int32_t kind, int32_t x, int32_t y, int32_t w, int32_t h,
                       double corner_radius, bool has_shadow,
                       const TSCanvasCallbacks *cbs, void *user_data) {
    ts_app_init();
    @autoreleasepool {
        NSRect frame = cg_px_to_ns_rect(x, y, w > 0 ? w : 1, h > 0 ? h : 1);
        NSWindowStyleMask mask = NSWindowStyleMaskBorderless;
        if (kind == TS_WIN_TOAST) {
            mask |= NSWindowStyleMaskNonactivatingPanel;
        }

        TSCanvasWindow *win = [[TSCanvasWindow alloc] initWithContentRect:frame
                                                                styleMask:mask
                                                                  backing:NSBackingStoreBuffered
                                                                    defer:NO];
        win.releasedWhenClosed = NO;
        win.opaque = NO;
        win.backgroundColor = [NSColor clearColor];
        win.hasShadow = has_shadow;
        win.hidesOnDeactivate = NO;
        win.delegate = win;
        win.userData = user_data;
        if (cbs) win.cbs = *cbs;

        switch (kind) {
            case TS_WIN_OVERLAY:
                win.allowKey = YES;
                win.level = NSScreenSaverWindowLevel;
                win.animationBehavior = NSWindowAnimationBehaviorNone;
                win.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces | NSWindowCollectionBehaviorFullScreenAuxiliary;
                break;
            case TS_WIN_FLOATING:
                win.allowKey = YES;
                win.level = NSStatusWindowLevel;
                win.animationBehavior = NSWindowAnimationBehaviorNone;
                win.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces | NSWindowCollectionBehaviorFullScreenAuxiliary;
                break;
            case TS_WIN_POPUP:
                win.allowKey = YES;
                win.level = NSPopUpMenuWindowLevel;
                win.animationBehavior = NSWindowAnimationBehaviorNone;
                break;
            case TS_WIN_TOAST:
                win.allowKey = NO;
                win.floatingPanel = YES;
                win.becomesKeyOnlyIfNeeded = YES;
                win.level = NSStatusWindowLevel + 1;
                win.animationBehavior = NSWindowAnimationBehaviorNone;
                win.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces | NSWindowCollectionBehaviorFullScreenAuxiliary;
                break;
            case TS_WIN_CLICKTHROUGH:
                win.allowKey = NO;
                win.ignoresMouseEvents = YES;
                win.level = NSScreenSaverWindowLevel;
                win.animationBehavior = NSWindowAnimationBehaviorNone;
                win.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces | NSWindowCollectionBehaviorFullScreenAuxiliary;
                break;
            case TS_WIN_DIALOG:
            default:
                win.allowKey = YES;
                win.level = NSFloatingWindowLevel;
                win.animationBehavior = NSWindowAnimationBehaviorDefault;
                break;
        }

        TSCanvasView *view = [[TSCanvasView alloc] initWithFrame:NSMakeRect(0, 0, frame.size.width, frame.size.height)];
        view.owner = win;
        view.wantsLayer = YES;
        if (corner_radius > 0.0) {
            CGFloat s = (CGFloat)ts_backing_scale();
            view.layer.cornerRadius = corner_radius / s;
            view.layer.masksToBounds = YES;
        }
        win.canvasView = view;
        win.contentView = view;
        [win disableCursorRects];
        return (void *)CFBridgingRetain(win);
    }
}

void ts_canvas_show(void *win_ptr, bool activate) {
    if (!win_ptr) return;
    @autoreleasepool {
        TSCanvasWindow *win = (__bridge TSCanvasWindow *)win_ptr;
        if (activate) {
            [NSApp activateIgnoringOtherApps:YES];
            if (@available(macOS 14.0, *)) {
                [NSApp activate];
            }
            [win makeKeyAndOrderFront:nil];
            [win orderFrontRegardless];
            [win makeFirstResponder:win.canvasView];
        } else {
            [win orderFrontRegardless];
        }
    }
}

void ts_canvas_set_frame(void *win_ptr, int32_t x, int32_t y, int32_t w, int32_t h) {
    if (!win_ptr) return;
    @autoreleasepool {
        TSCanvasWindow *win = (__bridge TSCanvasWindow *)win_ptr;
        NSRect frame = cg_px_to_ns_rect(x, y, w > 0 ? w : 1, h > 0 ? h : 1);
        [win setFrame:frame display:YES];
    }
}

void ts_canvas_set_opacity(void *win_ptr, double alpha) {
    if (!win_ptr) return;
    @autoreleasepool {
        TSCanvasWindow *win = (__bridge TSCanvasWindow *)win_ptr;
        win.alphaValue = alpha;
    }
}

void ts_canvas_invalidate(void *win_ptr) {
    if (!win_ptr) return;
    @autoreleasepool {
        TSCanvasWindow *win = (__bridge TSCanvasWindow *)win_ptr;
        [win.canvasView setNeedsDisplay:YES];
    }
}

void ts_canvas_invalidate_rect(void *win_ptr, int32_t x, int32_t y, int32_t w, int32_t h) {
    if (!win_ptr) return;
    @autoreleasepool {
        TSCanvasWindow *win = (__bridge TSCanvasWindow *)win_ptr;
        CGFloat s = (CGFloat)ts_backing_scale();
        [win.canvasView setNeedsDisplayInRect:NSMakeRect(x / s, y / s, w / s, h / s)];
    }
}

void ts_canvas_start_timer(void *win_ptr, uint32_t interval_ms) {
    if (!win_ptr) return;
    @autoreleasepool {
        TSCanvasWindow *win = (__bridge TSCanvasWindow *)win_ptr;
        if (win.timer) [win.timer invalidate];
        win.timer = [NSTimer scheduledTimerWithTimeInterval:(double)interval_ms / 1000.0
                                                     target:win
                                                   selector:@selector(timerFired:)
                                                   userInfo:nil
                                                    repeats:YES];
    }
}

void ts_canvas_stop_timer(void *win_ptr) {
    if (!win_ptr) return;
    @autoreleasepool {
        TSCanvasWindow *win = (__bridge TSCanvasWindow *)win_ptr;
        if (win.timer) {
            [win.timer invalidate];
            win.timer = nil;
        }
    }
}

void ts_canvas_close(void *win_ptr) {
    if (!win_ptr) return;
    @autoreleasepool {
        TSCanvasWindow *win = (TSCanvasWindow *)CFBridgingRelease(win_ptr);
        if (win.timer) {
            [win.timer invalidate];
            win.timer = nil;
        }
        win.closedNotified = YES; // programmatic close; don't re-enter on_close unless needed
        [win close];
    }
}

void ts_canvas_begin_edit(void *win_ptr, int32_t x, int32_t y, int32_t w, int32_t h,
                          int32_t font_px, uint32_t text_argb, uint32_t bg_argb) {
    if (!win_ptr) return;
    @autoreleasepool {
        TSCanvasWindow *win = (__bridge TSCanvasWindow *)win_ptr;
        TSCanvasView *view = win.canvasView;
        if (view.inlineEdit) {
            [view.inlineEdit removeFromSuperview];
            view.inlineEdit = nil;
        }
        CGFloat s = (CGFloat)ts_backing_scale();
        NSTextField *tf = [[NSTextField alloc] initWithFrame:NSMakeRect(x / s, y / s, w / s, h / s)];
        tf.font = make_ui_font((CGFloat)font_px / s, NO);
        tf.textColor = ns_color_argb(text_argb);
        tf.backgroundColor = ns_color_argb(bg_argb);
        tf.drawsBackground = YES;
        tf.bordered = YES;
        tf.focusRingType = NSFocusRingTypeNone;
        tf.delegate = view;
        [view addSubview:tf];
        [win makeFirstResponder:tf];
        view.inlineEdit = tf;
    }
}

char *ts_canvas_end_edit(void *win_ptr) {
    if (!win_ptr) return NULL;
    @autoreleasepool {
        TSCanvasWindow *win = (__bridge TSCanvasWindow *)win_ptr;
        TSCanvasView *view = win.canvasView;
        if (!view.inlineEdit) return NULL;
        NSString *val = view.inlineEdit.stringValue ?: @"";
        const char *utf8 = [val UTF8String];
        char *res = utf8 ? strdup(utf8) : NULL;
        [view.inlineEdit removeFromSuperview];
        view.inlineEdit = nil;
        [win makeFirstResponder:view];
        return res;
    }
}

bool ts_canvas_has_edit(void *win_ptr) {
    if (!win_ptr) return false;
    TSCanvasWindow *win = (__bridge TSCanvasWindow *)win_ptr;
    return win.canvasView.inlineEdit != nil;
}

@interface TSPopupMenuHandler : NSObject
@property (nonatomic, assign) int32_t picked;
@end

@implementation TSPopupMenuHandler
- (void)pickedItem:(NSMenuItem *)item {
    self.picked = (int32_t)item.tag;
}
@end

int32_t ts_popup_context_menu(void *win_ptr, const char *const *items, int32_t count) {
    if (!win_ptr || !items || count <= 0) return 0;
    @autoreleasepool {
        TSCanvasWindow *win = (__bridge TSCanvasWindow *)win_ptr;
        TSPopupMenuHandler *handler = [[TSPopupMenuHandler alloc] init];
        NSMenu *menu = [[NSMenu alloc] initWithTitle:@""];
        for (int32_t i = 0; i < count; i++) {
            NSString *title = [NSString stringWithUTF8String:items[i]] ?: @"";
            NSMenuItem *mi = [[NSMenuItem alloc] initWithTitle:title
                                                        action:@selector(pickedItem:)
                                                 keyEquivalent:@""];
            mi.target = handler;
            mi.tag = i + 1;
            [menu addItem:mi];
        }
        NSPoint loc = [win mouseLocationOutsideOfEventStream];
        [menu popUpMenuPositioningItem:nil atLocation:loc inView:win.canvasView];
        return handler.picked;
    }
}

// ============================================================================
// Quartz Drawing Primitives (Operating in TermShot Physical Pixels)
// ============================================================================

void *ts_image_create(const uint32_t *bgra, int32_t w, int32_t h) {
    if (!bgra || w <= 0 || h <= 0) return NULL;
    CGColorSpaceRef cs = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    CGContextRef ctx = CGBitmapContextCreate(
        (void *)bgra, (size_t)w, (size_t)h, 8, (size_t)w * 4, cs,
        kCGBitmapByteOrder32Little | kCGImageAlphaPremultipliedFirst);
    CGImageRef img = ctx ? CGBitmapContextCreateImage(ctx) : NULL;
    if (ctx) CGContextRelease(ctx);
    CGColorSpaceRelease(cs);
    return (void *)img;
}

void ts_image_free(void *img) {
    if (img) CGImageRelease((CGImageRef)img);
}

void ts_ctx_draw_image(void *ctx_ptr, void *img_ptr,
                       int32_t dx, int32_t dy, int32_t dw, int32_t dh,
                       int32_t sx, int32_t sy, int32_t sw, int32_t sh,
                       int32_t full_w, int32_t full_h) {
    if (!ctx_ptr || !img_ptr || dw <= 0 || dh <= 0 || sw <= 0 || sh <= 0) return;
    CGContextRef ctx = (CGContextRef)ctx_ptr;
    CGImageRef img = (CGImageRef)img_ptr;
    CGImageRef sub = NULL;
    if (sx == 0 && sy == 0 && sw == full_w && sh == full_h) {
        sub = CGImageRetain(img);
    } else {
        sub = CGImageCreateWithImageInRect(img, CGRectMake(sx, sy, sw, sh));
    }
    if (!sub) return;
    CGContextSaveGState(ctx);
    CGContextSetInterpolationQuality(ctx, kCGInterpolationHigh);
    CGContextTranslateCTM(ctx, (CGFloat)dx, (CGFloat)(dy + dh));
    CGContextScaleCTM(ctx, 1.0, -1.0);
    CGContextDrawImage(ctx, CGRectMake(0, 0, (CGFloat)dw, (CGFloat)dh), sub);
    CGContextRestoreGState(ctx);
    CGImageRelease(sub);
}

void ts_ctx_draw_bgra(void *ctx_ptr, const uint32_t *bgra, int32_t w, int32_t h,
                      int32_t dx, int32_t dy, int32_t dw, int32_t dh) {
    void *img = ts_image_create(bgra, w, h);
    if (!img) return;
    ts_ctx_draw_image(ctx_ptr, img, dx, dy, dw, dh, 0, 0, w, h, w, h);
    ts_image_free(img);
}

void ts_ctx_fill_rect(void *ctx_ptr, int32_t x, int32_t y, int32_t w, int32_t h, uint32_t argb) {
    if (!ctx_ptr || w <= 0 || h <= 0) return;
    CGContextRef ctx = (CGContextRef)ctx_ptr;
    set_ctx_fill_argb(ctx, argb);
    CGContextFillRect(ctx, CGRectMake(x, y, w, h));
}

void ts_ctx_stroke_rect(void *ctx_ptr, int32_t x, int32_t y, int32_t w, int32_t h, uint32_t argb, int32_t width) {
    if (!ctx_ptr || w <= 0 || h <= 0) return;
    CGContextRef ctx = (CGContextRef)ctx_ptr;
    CGFloat lw = (CGFloat)(width > 0 ? width : 1);
    set_ctx_stroke_argb(ctx, argb);
    CGContextSetLineWidth(ctx, lw);
    CGContextStrokeRect(ctx, CGRectMake(x + lw * 0.5, y + lw * 0.5, MAX(1.0, w - lw), MAX(1.0, h - lw)));
}

void ts_ctx_round_rect(void *ctx_ptr, int32_t x, int32_t y, int32_t w, int32_t h, int32_t radius,
                       uint32_t fill_argb, uint32_t stroke_argb, int32_t stroke_w) {
    if (!ctx_ptr || w <= 0 || h <= 0) return;
    CGContextRef ctx = (CGContextRef)ctx_ptr;
    CGFloat r = MIN((CGFloat)radius, MIN(w * 0.5, h * 0.5));
    CGRect rect = CGRectMake(x, y, w, h);
    CGPathRef path = CGPathCreateWithRoundedRect(rect, r, r, NULL);
    if ((fill_argb >> 24) > 0) {
        set_ctx_fill_argb(ctx, fill_argb);
        CGContextAddPath(ctx, path);
        CGContextFillPath(ctx);
    }
    if ((stroke_argb >> 24) > 0 && stroke_w > 0) {
        CGFloat sw = (CGFloat)stroke_w;
        CGRect srect = CGRectInset(rect, sw * 0.5, sw * 0.5);
        CGFloat sr = MAX(0.0, r - sw * 0.5);
        CGPathRef spath = CGPathCreateWithRoundedRect(srect, sr, sr, NULL);
        set_ctx_stroke_argb(ctx, stroke_argb);
        CGContextSetLineWidth(ctx, sw);
        CGContextAddPath(ctx, spath);
        CGContextStrokePath(ctx);
        CGPathRelease(spath);
    }
    CGPathRelease(path);
}

void ts_ctx_fill_ellipse(void *ctx_ptr, int32_t x, int32_t y, int32_t w, int32_t h, uint32_t argb) {
    if (!ctx_ptr || w <= 0 || h <= 0) return;
    CGContextRef ctx = (CGContextRef)ctx_ptr;
    set_ctx_fill_argb(ctx, argb);
    CGContextFillEllipseInRect(ctx, CGRectMake(x, y, w, h));
}

void ts_ctx_stroke_ellipse(void *ctx_ptr, int32_t x, int32_t y, int32_t w, int32_t h, uint32_t argb, int32_t width) {
    if (!ctx_ptr || w <= 0 || h <= 0) return;
    CGContextRef ctx = (CGContextRef)ctx_ptr;
    set_ctx_stroke_argb(ctx, argb);
    CGContextSetLineWidth(ctx, (CGFloat)(width > 0 ? width : 1));
    CGContextStrokeEllipseInRect(ctx, CGRectMake(x, y, w, h));
}

void ts_ctx_line(void *ctx_ptr, double x0, double y0, double x1, double y1, uint32_t argb, double width) {
    if (!ctx_ptr) return;
    CGContextRef ctx = (CGContextRef)ctx_ptr;
    set_ctx_stroke_argb(ctx, argb);
    CGContextSetLineWidth(ctx, (CGFloat)(width > 0.5 ? width : 1.0));
    CGContextSetLineCap(ctx, kCGLineCapRound);
    CGContextMoveToPoint(ctx, (CGFloat)x0, (CGFloat)y0);
    CGContextAddLineToPoint(ctx, (CGFloat)x1, (CGFloat)y1);
    CGContextStrokePath(ctx);
}

void ts_ctx_polyline(void *ctx_ptr, const int32_t *xy_pairs, int32_t count, uint32_t argb, double width) {
    if (!ctx_ptr || !xy_pairs || count < 2) return;
    CGContextRef ctx = (CGContextRef)ctx_ptr;
    set_ctx_stroke_argb(ctx, argb);
    CGContextSetLineWidth(ctx, (CGFloat)(width > 0.5 ? width : 1.0));
    CGContextSetLineCap(ctx, kCGLineCapRound);
    CGContextSetLineJoin(ctx, kCGLineJoinRound);
    CGContextMoveToPoint(ctx, (CGFloat)xy_pairs[0], (CGFloat)xy_pairs[1]);
    for (int32_t i = 1; i < count; i++) {
        CGContextAddLineToPoint(ctx, (CGFloat)xy_pairs[i * 2], (CGFloat)xy_pairs[i * 2 + 1]);
    }
    CGContextStrokePath(ctx);
}

void ts_ctx_polygon_fill(void *ctx_ptr, const double *xy_pairs, int32_t count, uint32_t argb) {
    if (!ctx_ptr || !xy_pairs || count < 3) return;
    CGContextRef ctx = (CGContextRef)ctx_ptr;
    set_ctx_fill_argb(ctx, argb);
    CGContextMoveToPoint(ctx, (CGFloat)xy_pairs[0], (CGFloat)xy_pairs[1]);
    for (int32_t i = 1; i < count; i++) {
        CGContextAddLineToPoint(ctx, (CGFloat)xy_pairs[i * 2], (CGFloat)xy_pairs[i * 2 + 1]);
    }
    CGContextClosePath(ctx);
    CGContextFillPath(ctx);
}

void ts_ctx_draw_text(void *ctx_ptr, int32_t x, int32_t y, int32_t w, int32_t h,
                      const char *text, uint32_t argb, int32_t px, bool bold,
                      int32_t align, bool wrap) {
    if (!ctx_ptr || !text || !*text || w <= 0 || h <= 0) return;
    @autoreleasepool {
        NSString *str = [NSString stringWithUTF8String:text];
        if (!str || str.length == 0) return;
        NSMutableParagraphStyle *ps = [[NSMutableParagraphStyle alloc] init];
        if (align == 1) ps.alignment = NSTextAlignmentCenter;
        else if (align == 2) ps.alignment = NSTextAlignmentRight;
        else ps.alignment = NSTextAlignmentLeft;
        ps.lineBreakMode = wrap ? NSLineBreakByWordWrapping : NSLineBreakByTruncatingTail;

        NSFont *font = make_ui_font((CGFloat)(px > 0 ? px : 12), bold);
        NSDictionary *attrs = @{
            NSFontAttributeName: font,
            NSForegroundColorAttributeName: ns_color_argb(argb),
            NSParagraphStyleAttributeName: ps,
        };

        CGContextRef ctx = (CGContextRef)ctx_ptr;
        [NSGraphicsContext saveGraphicsState];
        NSGraphicsContext *nsCtx = [NSGraphicsContext graphicsContextWithCGContext:ctx flipped:YES];
        [NSGraphicsContext setCurrentContext:nsCtx];
        if (wrap) {
            [str drawWithRect:NSMakeRect(x, y, w, h)
                      options:(NSStringDrawingUsesLineFragmentOrigin | NSStringDrawingTruncatesLastVisibleLine)
                   attributes:attrs];
        } else {
            NSSize sz = [str sizeWithAttributes:attrs];
            CGFloat ty = y + MAX(0.0, ((CGFloat)h - sz.height) * 0.5);
            [str drawInRect:NSMakeRect(x, ty, w, sz.height + 2.0) withAttributes:attrs];
        }
        [NSGraphicsContext restoreGraphicsState];
    }
}

// ============================================================================
// Dialogs (Folder Picker & Confirmation Alert)
// ============================================================================

char *ts_pick_folder(const char *title, const char *initial_dir) {
    ts_app_init();
    @autoreleasepool {
        NSOpenPanel *panel = [NSOpenPanel openPanel];
        panel.canChooseFiles = NO;
        panel.canChooseDirectories = YES;
        panel.canCreateDirectories = YES;
        panel.allowsMultipleSelection = NO;
        if (title) panel.prompt = @"选择";
        if (title) panel.message = [NSString stringWithUTF8String:title];
        if (initial_dir && *initial_dir) {
            NSString *path = [NSString stringWithUTF8String:initial_dir];
            if (path && [[NSFileManager defaultManager] fileExistsAtPath:path]) {
                panel.directoryURL = [NSURL fileURLWithPath:path];
            }
        }
        NSModalResponse resp = [panel runModal];
        if (resp == NSModalResponseOK && panel.URL) {
            const char *p = [panel.URL.path UTF8String];
            return p ? strdup(p) : NULL;
        }
        return NULL;
    }
}

bool ts_confirm_dialog(const char *title, const char *message) {
    ts_app_init();
    @autoreleasepool {
        NSAlert *alert = [[NSAlert alloc] init];
        alert.alertStyle = NSAlertStyleWarning;
        alert.messageText = title ? [NSString stringWithUTF8String:title] : @"确认";
        alert.informativeText = message ? [NSString stringWithUTF8String:message] : @"";
        [alert addButtonWithTitle:@"确定"];
        [alert addButtonWithTitle:@"取消"];
        return [alert runModal] == NSAlertFirstButtonReturn;
    }
}

// ============================================================================
// Native Rich Translation / OCR Result Window
// ============================================================================

@interface TSCardView : NSView
@property (nonatomic, strong) NSColor *bgColor;
@property (nonatomic, strong) NSColor *borderColor;
@property (nonatomic, assign) CGFloat cornerRadius;
@property (nonatomic, assign) BOOL leftAccent;
@property (nonatomic, assign) BOOL useFlippedCoords;
@end

@implementation TSCardView
- (BOOL)isFlipped { return self.useFlippedCoords; }
- (void)drawRect:(NSRect)dirtyRect {
    NSBezierPath *path = [NSBezierPath bezierPathWithRoundedRect:NSInsetRect(self.bounds, 0.5, 0.5)
                                                         xRadius:self.cornerRadius
                                                         yRadius:self.cornerRadius];
    [self.bgColor setFill];
    [path fill];
    if (self.borderColor) {
        [self.borderColor setStroke];
        path.lineWidth = 1.0;
        [path stroke];
    }
    if (self.leftAccent) {
        NSRect bar = NSMakeRect(6, 12, 3, MAX(12, self.bounds.size.height - 24));
        [[ns_color_argb(0xFF2DE2A8) colorWithAlphaComponent:1.0] setFill];
        [[NSBezierPath bezierPathWithRoundedRect:bar xRadius:1.5 yRadius:1.5] fill];
    }
}
@end

@interface TSStyledButton : NSButton
@property (nonatomic, assign) BOOL primary;
@property (nonatomic, assign) BOOL danger;
@property (nonatomic, assign) BOOL hovered;
@property (nonatomic, strong) NSTrackingArea *ta;
@end

@implementation TSStyledButton
- (void)updateTrackingAreas {
    [super updateTrackingAreas];
    if (self.ta) [self removeTrackingArea:self.ta];
    self.ta = [[NSTrackingArea alloc] initWithRect:self.bounds
                                           options:(NSTrackingMouseEnteredAndExited | NSTrackingActiveAlways | NSTrackingInVisibleRect)
                                             owner:self
                                          userInfo:nil];
    [self addTrackingArea:self.ta];
}
- (void)mouseEntered:(NSEvent *)event { self.hovered = YES; [self setNeedsDisplay:YES]; }
- (void)mouseExited:(NSEvent *)event { self.hovered = NO; [self setNeedsDisplay:YES]; }
- (void)resetCursorRects {
    [self addCursorRect:self.bounds cursor:[NSCursor pointingHandCursor]];
}
- (void)drawRect:(NSRect)dirtyRect {
    NSColor *bg, *border, *fg;
    if (!self.enabled) {
        bg = ns_color_argb(0xFF1A212B);
        border = ns_color_argb(0xFF2A3340);
        fg = ns_color_argb(0xFF8B94A0);
    } else if (self.primary) {
        bg = self.highlighted ? ns_color_argb(0xFF22B082) : (self.hovered ? ns_color_argb(0xFF5EF0C0) : ns_color_argb(0xFF2DE2A8));
        border = bg;
        fg = ns_color_argb(0xFF06221B);
    } else if (self.danger) {
        bg = self.hovered ? ns_color_argb(0xFF521A1A) : ns_color_argb(0xFF3B1414);
        border = ns_color_argb(0xFFF07178);
        fg = self.hovered ? ns_color_argb(0xFFFF9999) : ns_color_argb(0xFFF07178);
    } else {
        bg = self.highlighted ? ns_color_argb(0xFF121820) : (self.hovered ? ns_color_argb(0xFF232D3B) : ns_color_argb(0xFF1A212B));
        border = (self.hovered || self.highlighted) ? ns_color_argb(0xFF2DE2A8) : ns_color_argb(0xFF2A3340);
        fg = ns_color_argb(0xFFE6EDF3);
    }
    NSBezierPath *path = [NSBezierPath bezierPathWithRoundedRect:NSInsetRect(self.bounds, 0.5, 0.5) xRadius:6 yRadius:6];
    [bg setFill];
    [path fill];
    [border setStroke];
    path.lineWidth = 1.0;
    [path stroke];

    NSMutableParagraphStyle *ps = [[NSMutableParagraphStyle alloc] init];
    ps.alignment = NSTextAlignmentCenter;
    NSDictionary *attrs = @{
        NSFontAttributeName: make_ui_font(12.5, self.primary),
        NSForegroundColorAttributeName: fg,
        NSParagraphStyleAttributeName: ps,
    };
    NSString *title = self.title ?: @"";
    NSSize sz = [title sizeWithAttributes:attrs];
    CGFloat ty = (self.bounds.size.height - sz.height) * 0.5 + (self.highlighted ? 0.5 : 0.0);
    [title drawInRect:NSMakeRect(4, ty, self.bounds.size.width - 8, sz.height + 2) withAttributes:attrs];
}
@end

@interface TSTransWindow : NSPanel <NSWindowDelegate>
@property (nonatomic, assign) void (*onCopy)(void *);
@property (nonatomic, assign) void (*onRetry)(void *);
@property (nonatomic, assign) void (*onClose)(void *);
@property (nonatomic, assign) void *userData;

@property (nonatomic, strong) NSTextField *titleLabel;
@property (nonatomic, strong) NSTextField *subtitleLabel;
@property (nonatomic, strong) TSCardView *statusBadge;
@property (nonatomic, strong) NSTextField *statusLabel;

@property (nonatomic, strong) TSCardView *sourceCard;
@property (nonatomic, strong) NSTextField *sourceHeader;
@property (nonatomic, strong) NSTextField *sourceCount;
@property (nonatomic, strong) NSScrollView *sourceScroll;
@property (nonatomic, strong) NSTextView *sourceTextView;
@property (nonatomic, strong) NSImageView *sourceImageView;

@property (nonatomic, strong) TSCardView *resultCard;
@property (nonatomic, strong) NSTextField *resultHeader;
@property (nonatomic, strong) NSTextField *resultCount;
@property (nonatomic, strong) NSScrollView *resultScroll;
@property (nonatomic, strong) NSTextView *resultTextView;
@property (nonatomic, strong) NSView *loadingView;
@property (nonatomic, strong) NSArray<NSView *> *skeletonBars;
@property (nonatomic, strong) NSView *errorView;
@property (nonatomic, strong) NSTextField *errorTitle;
@property (nonatomic, strong) NSTextField *errorMsg;

@property (nonatomic, strong) NSView *actionBar;
@property (nonatomic, strong) NSTextField *hintLabel;
@property (nonatomic, strong) TSStyledButton *resultCopyBtn;
@property (nonatomic, strong) TSStyledButton *retryBtn;
@property (nonatomic, strong) TSStyledButton *closeBtn;

@property (nonatomic, strong) NSTimer *pulseTimer;
@property (nonatomic, assign) int32_t pulsePhase;
@property (nonatomic, assign) int32_t viewState;
@end

@implementation TSTransWindow
- (void)layoutViews {
    NSRect b = self.contentView.bounds;
    CGFloat w = b.size.width;
    CGFloat h = b.size.height;
    CGFloat margin = 16.0;
    CGFloat headerH = 68.0;
    CGFloat actionH = 62.0;
    CGFloat gap = 12.0;

    self.titleLabel.frame = NSMakeRect(18, h - 38, w - 140, 26);
    self.subtitleLabel.frame = NSMakeRect(18, h - 61, w - 140, 20);
    self.statusBadge.frame = NSMakeRect(w - 100, h - 46, 82, 26);
    self.statusLabel.frame = NSMakeRect(0, 4, 82, 18);

    CGFloat cardsTop = h - headerH - 4.0;
    CGFloat cardsBottom = actionH + 12.0;
    CGFloat availH = MAX(120.0, cardsTop - cardsBottom - gap);
    CGFloat sourceH = floor(availH * 2.0 / 5.0);
    CGFloat resultH = availH - sourceH;
    CGFloat cardW = MAX(200.0, w - margin * 2.0);

    self.sourceCard.frame = NSMakeRect(margin, cardsTop - sourceH, cardW, sourceH);
    self.resultCard.frame = NSMakeRect(margin, cardsBottom, cardW, resultH);

    self.sourceHeader.frame = NSMakeRect(14, 10, cardW - 110, 24);
    self.sourceCount.frame = NSMakeRect(cardW - 114, 10, 100, 24);
    NSRect srcBody = NSMakeRect(14, 40, cardW - 28, MAX(20, sourceH - 54));
    self.sourceScroll.frame = srcBody;
    self.sourceImageView.frame = srcBody;

    self.resultHeader.frame = NSMakeRect(18, 10, cardW - 110, 24);
    self.resultCount.frame = NSMakeRect(cardW - 114, 10, 100, 24);
    NSRect resBody = NSMakeRect(14, 40, cardW - 28, MAX(20, resultH - 54));
    self.resultScroll.frame = resBody;
    self.loadingView.frame = resBody;
    CGFloat widths[] = { 0.72, 0.92, 0.84, 0.64 };
    for (NSUInteger i = 0; i < self.skeletonBars.count; i++) {
        CGFloat by = MAX(32.0, resBody.size.height - 20.0 - (CGFloat)i * 26.0);
        self.skeletonBars[i].frame = NSMakeRect(0, by, resBody.size.width * widths[i], 11);
    }
    self.errorView.frame = resBody;
    self.errorTitle.frame = NSMakeRect(0, MAX(0.0, resBody.size.height - 34.0), resBody.size.width, 26);
    self.errorMsg.frame = NSMakeRect(0, 4, resBody.size.width, MAX(20.0, resBody.size.height - 44.0));

    self.actionBar.frame = NSMakeRect(0, 0, w, actionH);
    CGFloat btnH = 34.0;
    CGFloat btnY = (actionH - btnH) * 0.5;
    CGFloat closeW = 88.0;
    CGFloat copyW = 112.0;
    self.closeBtn.frame = NSMakeRect(w - margin - closeW, btnY, closeW, btnH);
    self.resultCopyBtn.frame = NSMakeRect(self.closeBtn.frame.origin.x - 10 - copyW, btnY, copyW, btnH);
    self.retryBtn.frame = self.resultCopyBtn.frame;
    self.hintLabel.frame = NSMakeRect(18, 18, MAX(80, self.resultCopyBtn.frame.origin.x - 28), 26);
}

- (void)windowDidResize:(NSNotification *)notification {
    [self layoutViews];
}

- (void)windowWillClose:(NSNotification *)notification {
    if (self.pulseTimer) {
        [self.pulseTimer invalidate];
        self.pulseTimer = nil;
    }
    if (self.onClose) {
        void (*fn)(void *) = self.onClose;
        self.onClose = NULL;
        fn(self.userData);
    }
}

- (void)onCopyClicked:(id)sender {
    if (self.onCopy) self.onCopy(self.userData);
}

- (void)onRetryClicked:(id)sender {
    if (self.onRetry) self.onRetry(self.userData);
}

- (void)onCloseClicked:(id)sender {
    [self close];
}

- (void)pulseTick:(NSTimer *)t {
    self.pulsePhase = (self.pulsePhase + 1) % 20;
    NSColor *col = (self.pulsePhase < 10) ? ns_color_argb(0xFF232D3B) : ns_color_argb(0xFF2A3340);
    for (NSView *bar in self.skeletonBars) {
        bar.layer.backgroundColor = col.CGColor;
    }
}

- (BOOL)performKeyEquivalent:(NSEvent *)event {
    uint32_t vk = mac_keycode_to_vk(event.keyCode);
    NSEventModifierFlags mods = event.modifierFlags;
    if (vk == 0x1B) { // Esc
        [self close];
        return YES;
    }
    BOOL cmdOrCtrl = (mods & (NSEventModifierFlagCommand | NSEventModifierFlagControl)) != 0;
    if (cmdOrCtrl && vk == 0x0D && self.viewState == 1) { // Cmd/Ctrl + Enter
        [self onCopyClicked:nil];
        return YES;
    }
    if (cmdOrCtrl && vk == 'R' && self.viewState == 2) { // Cmd/Ctrl + R
        [self onRetryClicked:nil];
        return YES;
    }
    return [super performKeyEquivalent:event];
}
@end

static NSTextField *make_label(NSString *text, NSFont *font, NSColor *color, NSTextAlignment align) {
    NSTextField *lbl = [NSTextField labelWithString:text ?: @""];
    lbl.font = font;
    lbl.textColor = color;
    lbl.alignment = align;
    lbl.backgroundColor = [NSColor clearColor];
    lbl.drawsBackground = NO;
    return lbl;
}

static NSScrollView *make_readonly_textview(NSTextView **out_tv) {
    NSScrollView *sv = [[NSScrollView alloc] initWithFrame:NSMakeRect(0, 0, 100, 100)];
    sv.hasVerticalScroller = YES;
    sv.hasHorizontalScroller = NO;
    sv.borderType = NSNoBorder;
    sv.drawsBackground = NO;
    NSTextView *tv = [[NSTextView alloc] initWithFrame:NSMakeRect(0, 0, 100, 100)];
    tv.editable = NO;
    tv.selectable = YES;
    tv.drawsBackground = NO;
    tv.textColor = ns_color_argb(0xFFE6EDF3);
    tv.font = make_ui_font(13.5, NO);
    tv.minSize = NSMakeSize(0.0, 0.0);
    tv.maxSize = NSMakeSize(1e7, 1e7);
    tv.verticallyResizable = YES;
    tv.horizontallyResizable = NO;
    tv.autoresizingMask = NSViewWidthSizable;
    tv.textContainer.containerSize = NSMakeSize(100.0, 1e7);
    tv.textContainer.widthTracksTextView = YES;
    sv.documentView = tv;
    if (out_tv) *out_tv = tv;
    return sv;
}

void *ts_trans_create(int32_t x, int32_t y, int32_t w, int32_t h,
                      void (*on_copy)(void *), void (*on_retry)(void *),
                      void (*on_close)(void *), void *user_data) {
    ts_app_init();
    @autoreleasepool {
        NSRect frame = cg_px_to_ns_rect(x, y, w > 0 ? w : 680, h > 0 ? h : 540);
        NSWindowStyleMask mask = NSWindowStyleMaskTitled | NSWindowStyleMaskClosable
                               | NSWindowStyleMaskMiniaturizable | NSWindowStyleMaskResizable;
        TSTransWindow *win = [[TSTransWindow alloc] initWithContentRect:frame
                                                              styleMask:mask
                                                                backing:NSBackingStoreBuffered
                                                                  defer:NO];
        win.releasedWhenClosed = NO;
        win.title = @"TermShot — AI 结果";
        win.level = NSFloatingWindowLevel;
        win.minSize = NSMakeSize(540, 460);
        win.delegate = win;
        win.onCopy = on_copy;
        win.onRetry = on_retry;
        win.onClose = on_close;
        win.userData = user_data;
        if (@available(macOS 10.14, *)) {
            win.appearance = [NSAppearance appearanceNamed:NSAppearanceNameDarkAqua];
        }
        win.backgroundColor = ns_color_argb(0xFF12161C);

        NSView *root = win.contentView;
        root.wantsLayer = YES;
        root.layer.backgroundColor = ns_color_argb(0xFF12161C).CGColor;

        win.titleLabel = make_label(@"AI 结果", make_ui_font(18, YES), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
        win.subtitleLabel = make_label(@"", make_ui_font(11, NO), ns_color_argb(0xFF8B94A0), NSTextAlignmentLeft);
        [root addSubview:win.titleLabel];
        [root addSubview:win.subtitleLabel];

        win.statusBadge = [[TSCardView alloc] initWithFrame:NSZeroRect];
        win.statusBadge.useFlippedCoords = YES;
        win.statusBadge.cornerRadius = 13;
        win.statusBadge.bgColor = ns_color_argb(0xFF0D1117);
        win.statusBadge.borderColor = ns_color_argb(0xFFF5B942);
        win.statusLabel = make_label(@"● 处理中", make_ui_font(11, NO), ns_color_argb(0xFFF5B942), NSTextAlignmentCenter);
        [win.statusBadge addSubview:win.statusLabel];
        [root addSubview:win.statusBadge];

        // Source Card
        win.sourceCard = [[TSCardView alloc] initWithFrame:NSZeroRect];
        win.sourceCard.useFlippedCoords = YES;
        win.sourceCard.cornerRadius = 10;
        win.sourceCard.bgColor = ns_color_argb(0xFF1A212B);
        win.sourceCard.borderColor = ns_color_argb(0xFF2A3340);
        win.sourceHeader = make_label(@"原文", make_ui_font(13, YES), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
        win.sourceCount = make_label(@"0 字符", make_ui_font(11, NO), ns_color_argb(0xFF8B94A0), NSTextAlignmentRight);
        NSTextView *stv = nil;
        win.sourceScroll = make_readonly_textview(&stv);
        win.sourceTextView = stv;
        win.sourceImageView = [[NSImageView alloc] initWithFrame:NSZeroRect];
        win.sourceImageView.imageScaling = NSImageScaleProportionallyDown;
        [win.sourceCard addSubview:win.sourceHeader];
        [win.sourceCard addSubview:win.sourceCount];
        [win.sourceCard addSubview:win.sourceScroll];
        [win.sourceCard addSubview:win.sourceImageView];
        [root addSubview:win.sourceCard];

        // Result Card
        win.resultCard = [[TSCardView alloc] initWithFrame:NSZeroRect];
        win.resultCard.useFlippedCoords = YES;
        win.resultCard.cornerRadius = 10;
        win.resultCard.bgColor = ns_color_argb(0xFF1A212B);
        win.resultCard.borderColor = ns_color_argb(0xFF2A3340);
        win.resultCard.leftAccent = YES;
        win.resultHeader = make_label(@"翻译结果", make_ui_font(13, YES), ns_color_argb(0xFF5EF0C0), NSTextAlignmentLeft);
        win.resultCount = make_label(@"0 字符", make_ui_font(11, NO), ns_color_argb(0xFF8B94A0), NSTextAlignmentRight);
        NSTextView *rtv = nil;
        win.resultScroll = make_readonly_textview(&rtv);
        win.resultTextView = rtv;
        [win.resultCard addSubview:win.resultHeader];
        [win.resultCard addSubview:win.resultCount];
        [win.resultCard addSubview:win.resultScroll];

        win.loadingView = [[NSView alloc] initWithFrame:NSZeroRect];
        NSMutableArray *bars = [NSMutableArray array];
        for (int i = 0; i < 4; i++) {
            NSView *bar = [[NSView alloc] initWithFrame:NSZeroRect];
            bar.wantsLayer = YES;
            bar.layer.cornerRadius = 5;
            bar.layer.backgroundColor = ns_color_argb(0xFF232D3B).CGColor;
            [win.loadingView addSubview:bar];
            [bars addObject:bar];
        }
        win.skeletonBars = bars;
        NSTextField *waitLbl = make_label(@"AI 正在处理，请稍候…", make_ui_font(11.5, NO), ns_color_argb(0xFF8B94A0), NSTextAlignmentLeft);
        waitLbl.frame = NSMakeRect(0, 8, 300, 22);
        [win.loadingView addSubview:waitLbl];
        [win.resultCard addSubview:win.loadingView];

        win.errorView = [[NSView alloc] initWithFrame:NSZeroRect];
        win.errorTitle = make_label(@"处理失败", make_ui_font(13, YES), ns_color_argb(0xFFF07178), NSTextAlignmentLeft);
        win.errorMsg = make_label(@"", make_ui_font(13, NO), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
        win.errorMsg.lineBreakMode = NSLineBreakByWordWrapping;
        win.errorMsg.maximumNumberOfLines = 0;
        [win.errorView addSubview:win.errorTitle];
        [win.errorView addSubview:win.errorMsg];
        [win.resultCard addSubview:win.errorView];
        [root addSubview:win.resultCard];

        // Action bar
        win.actionBar = [[NSView alloc] initWithFrame:NSZeroRect];
        win.actionBar.wantsLayer = YES;
        win.actionBar.layer.backgroundColor = ns_color_argb(0xFF141922).CGColor;
        win.hintLabel = make_label(@"", make_ui_font(11, NO), ns_color_argb(0xFF8B94A0), NSTextAlignmentLeft);
        win.resultCopyBtn = [[TSStyledButton alloc] initWithFrame:NSZeroRect];
        win.resultCopyBtn.primary = YES;
        win.resultCopyBtn.title = @"复制结果";
        win.resultCopyBtn.target = win;
        win.resultCopyBtn.action = @selector(onCopyClicked:);

        win.retryBtn = [[TSStyledButton alloc] initWithFrame:NSZeroRect];
        win.retryBtn.primary = YES;
        win.retryBtn.title = @"重新尝试";
        win.retryBtn.target = win;
        win.retryBtn.action = @selector(onRetryClicked:);

        win.closeBtn = [[TSStyledButton alloc] initWithFrame:NSZeroRect];
        win.closeBtn.title = @"关闭";
        win.closeBtn.target = win;
        win.closeBtn.action = @selector(onCloseClicked:);

        [win.actionBar addSubview:win.hintLabel];
        [win.actionBar addSubview:win.resultCopyBtn];
        [win.actionBar addSubview:win.retryBtn];
        [win.actionBar addSubview:win.closeBtn];
        [root addSubview:win.actionBar];

        [win layoutViews];
        return (void *)CFBridgingRetain(win);
    }
}

void ts_trans_update(void *win_ptr, int32_t kind, int32_t view_state,
                     const char *source_text, const uint32_t *img_bgra, int32_t img_w, int32_t img_h,
                     const char *result_text, const char *error_text, bool copied) {
    if (!win_ptr) return;
    @autoreleasepool {
        TSTransWindow *win = (__bridge TSTransWindow *)win_ptr;
        win.viewState = view_state;
        if (kind == 0) {
            win.title = @"TermShot — 文字识别";
            win.titleLabel.stringValue = @"文字识别";
            win.subtitleLabel.stringValue = @"从截图中提取可复制文本";
            win.sourceHeader.stringValue = @"原始截图";
            win.resultHeader.stringValue = @"识别结果";
        } else if (kind == 1) {
            win.title = @"TermShot — 截图翻译";
            win.titleLabel.stringValue = @"截图翻译";
            win.subtitleLabel.stringValue = @"识别截图内容并翻译";
            win.sourceHeader.stringValue = @"原始截图";
            win.resultHeader.stringValue = @"翻译结果";
        } else {
            win.title = @"TermShot — 剪贴板翻译";
            win.titleLabel.stringValue = @"剪贴板翻译";
            win.subtitleLabel.stringValue = @"自动识别语言 · 中英互译";
            win.sourceHeader.stringValue = @"原文";
            win.resultHeader.stringValue = @"翻译结果";
        }

        NSString *srcStr = source_text ? ([NSString stringWithUTF8String:source_text] ?: @"") : @"";
        win.sourceCount.stringValue = [NSString stringWithFormat:@"%lu 字符", (unsigned long)srcStr.length];
        if (img_bgra && img_w > 0 && img_h > 0) {
            win.sourceScroll.hidden = YES;
            win.sourceImageView.hidden = NO;
            win.sourceImageView.image = nsimage_from_bgra(img_bgra, img_w, img_h, NSMakeSize(img_w, img_h));
        } else {
            win.sourceImageView.hidden = YES;
            win.sourceScroll.hidden = NO;
            win.sourceTextView.string = srcStr;
        }

        NSString *resStr = result_text ? ([NSString stringWithUTF8String:result_text] ?: @"") : @"";
        win.resultCount.stringValue = [NSString stringWithFormat:@"%lu 字符", (unsigned long)resStr.length];
        win.resultTextView.string = resStr;

        NSString *errStr = error_text ? ([NSString stringWithUTF8String:error_text] ?: @"") : @"";
        win.errorMsg.stringValue = errStr;

        if (view_state == 0) { // Loading
            win.statusLabel.stringValue = @"● 处理中";
            win.statusLabel.textColor = ns_color_argb(0xFFF5B942);
            win.statusBadge.bgColor = ns_color_argb(0xFF0D1117);
            win.statusBadge.borderColor = ns_color_argb(0xFFF5B942);
            win.loadingView.hidden = NO;
            win.resultScroll.hidden = YES;
            win.errorView.hidden = YES;
            win.resultCopyBtn.hidden = YES;
            win.retryBtn.hidden = YES;
            win.hintLabel.stringValue = @"处理中可关闭窗口，完成后不会自动覆盖剪贴板";
            if (!win.pulseTimer) {
                win.pulseTimer = [NSTimer scheduledTimerWithTimeInterval:0.09
                                                                  target:win
                                                                selector:@selector(pulseTick:)
                                                                userInfo:nil
                                                                 repeats:YES];
            }
        } else if (view_state == 1) { // Success
            if (win.pulseTimer) { [win.pulseTimer invalidate]; win.pulseTimer = nil; }
            win.statusLabel.stringValue = @"● 已完成";
            win.statusLabel.textColor = ns_color_argb(0xFF5EF0C0);
            win.statusBadge.bgColor = ns_color_argb(0xFF0B3D33);
            win.statusBadge.borderColor = ns_color_argb(0xFF2DE2A8);
            win.loadingView.hidden = YES;
            win.resultScroll.hidden = NO;
            win.errorView.hidden = YES;
            win.resultCopyBtn.hidden = NO;
            win.resultCopyBtn.enabled = YES;
            win.resultCopyBtn.title = copied ? @"✓ 已复制" : @"复制结果";
            win.retryBtn.hidden = YES;
            win.hintLabel.stringValue = @"仅点击“复制结果”时写入剪贴板  ·  ⌘Enter";
        } else { // Error
            if (win.pulseTimer) { [win.pulseTimer invalidate]; win.pulseTimer = nil; }
            win.statusLabel.stringValue = @"● 失败";
            win.statusLabel.textColor = ns_color_argb(0xFFF07178);
            win.statusBadge.bgColor = ns_color_argb(0xFF321D24);
            win.statusBadge.borderColor = ns_color_argb(0xFFF07178);
            win.loadingView.hidden = YES;
            win.resultScroll.hidden = YES;
            win.errorView.hidden = NO;
            win.resultCopyBtn.hidden = YES;
            win.retryBtn.hidden = NO;
            win.hintLabel.stringValue = @"检查网络或 API 配置后可重新尝试  ·  ⌘R";
        }
        [win.statusBadge setNeedsDisplay:YES];
    }
}

void ts_trans_show(void *win_ptr) {
    if (!win_ptr) return;
    @autoreleasepool {
        TSTransWindow *win = (__bridge TSTransWindow *)win_ptr;
        [win orderFrontRegardless];
    }
}

void ts_trans_close(void *win_ptr) {
    if (!win_ptr) return;
    @autoreleasepool {
        TSTransWindow *win = (TSTransWindow *)CFBridgingRelease(win_ptr);
        if (win.pulseTimer) { [win.pulseTimer invalidate]; win.pulseTimer = nil; }
        win.onClose = NULL;
        [win close];
    }
}

// ============================================================================
// Native Rich Settings Window
// ============================================================================

typedef struct {
    const char *save_dir;
    bool quote_path;
    int32_t post_action_idx;
    bool start_at_login;
    const char *capture_hotkey_text;
    const char *translate_hotkey_text;
    const char *api_key;
    const char *model;
    const char *config_dir;
    bool has_screen_perm;
} TSSettingsInit;

typedef struct {
    char *save_dir;
    bool quote_path;
    int32_t post_action_idx;
    bool start_at_login;
    char *api_key;
    char *model;
} TSSettingsValues;

typedef struct {
    void (*on_browse)(void *user_data);
    void (*on_open_dir)(void *user_data);
    void (*on_open_config)(void *user_data);
    void (*on_record_hotkey)(int32_t target, void *user_data);
    void (*on_reset_hotkey)(int32_t target, void *user_data);
    void (*on_hotkey_input)(uint32_t vk, uint32_t mods, void *user_data);
    void (*on_test_api)(void *user_data);
    void (*on_open_perm)(void *user_data);
    void (*on_save)(void *user_data);
    void (*on_cancel)(void *user_data);
    void (*on_close)(void *user_data);
} TSSettingsCallbacks;

@interface TSSettingsTabButton : NSButton
@property (nonatomic, assign) BOOL activeTab;
@property (nonatomic, assign) BOOL hovered;
@property (nonatomic, copy) NSString *numStr;
@property (nonatomic, copy) NSString *labelStr;
@property (nonatomic, strong) NSTrackingArea *ta;
@end

@implementation TSSettingsTabButton
- (void)updateTrackingAreas {
    [super updateTrackingAreas];
    if (self.ta) [self removeTrackingArea:self.ta];
    self.ta = [[NSTrackingArea alloc] initWithRect:self.bounds
                                           options:(NSTrackingMouseEnteredAndExited | NSTrackingActiveAlways | NSTrackingInVisibleRect)
                                             owner:self
                                          userInfo:nil];
    [self addTrackingArea:self.ta];
}
- (void)mouseEntered:(NSEvent *)event { self.hovered = YES; [self setNeedsDisplay:YES]; }
- (void)mouseExited:(NSEvent *)event { self.hovered = NO; [self setNeedsDisplay:YES]; }
- (void)resetCursorRects {
    [self addCursorRect:self.bounds cursor:[NSCursor pointingHandCursor]];
}
- (void)drawRect:(NSRect)dirtyRect {
    if (self.activeTab) {
        NSBezierPath *p = [NSBezierPath bezierPathWithRoundedRect:NSInsetRect(self.bounds, 0.5, 0.5) xRadius:6 yRadius:6];
        [ns_color_argb(0xFF1A212B) setFill];
        [p fill];
        [ns_color_argb(0xFF2A3340) setStroke];
        p.lineWidth = 1.0;
        [p stroke];
        [ns_color_argb(0xFF2DE2A8) setFill];
        [[NSBezierPath bezierPathWithRoundedRect:NSMakeRect(2, 8, 3, self.bounds.size.height - 16) xRadius:1.5 yRadius:1.5] fill];
    } else if (self.hovered) {
        NSBezierPath *p = [NSBezierPath bezierPathWithRoundedRect:NSInsetRect(self.bounds, 0.5, 0.5) xRadius:6 yRadius:6];
        [ns_color_argb(0xFF232D3B) setFill];
        [p fill];
        [ns_color_argb(0xFF2A3340) setStroke];
        [p stroke];
    }
    NSColor *fg = self.activeTab ? ns_color_argb(0xFF5EF0C0) : (self.hovered ? ns_color_argb(0xFFE6EDF3) : ns_color_argb(0xFF8B94A0));
    NSString *display = [NSString stringWithFormat:@" %@   %@", self.numStr ?: @"", self.labelStr ?: @""];
    NSDictionary *attrs = @{
        NSFontAttributeName: make_ui_font(13, self.activeTab),
        NSForegroundColorAttributeName: fg,
    };
    NSSize sz = [display sizeWithAttributes:attrs];
    [display drawAtPoint:NSMakePoint(10, (self.bounds.size.height - sz.height) * 0.5) withAttributes:attrs];
}
@end

@interface TSSettingsWindow : NSWindow <NSWindowDelegate>
@property (nonatomic, assign) TSSettingsCallbacks cbs;
@property (nonatomic, assign) void *userData;
@property (nonatomic, assign) BOOL recordingHotkey;
@property (nonatomic, assign) NSInteger activeTab;

@property (nonatomic, strong) NSArray<TSSettingsTabButton *> *tabBtns;
@property (nonatomic, strong) NSArray<NSView *> *tabPages;
@property (nonatomic, strong) NSTextField *headerTitle;
@property (nonatomic, strong) NSTextField *headerSub;

// Tab 0
@property (nonatomic, strong) NSTextField *dirField;
@property (nonatomic, strong) NSButton *quoteCheck;
@property (nonatomic, strong) NSPopUpButton *actionPopup;
@property (nonatomic, strong) NSButton *startupCheck;

// Tab 1
@property (nonatomic, strong) NSTextField *hotkeyField;
@property (nonatomic, strong) TSStyledButton *recordBtn;
@property (nonatomic, strong) NSTextField *transHotkeyField;
@property (nonatomic, strong) TSStyledButton *transRecordBtn;

// Tab 2
@property (nonatomic, strong) NSSecureTextField *apiSecureField;
@property (nonatomic, strong) NSTextField *apiPlainField;
@property (nonatomic, strong) TSStyledButton *toggleKeyBtn;
@property (nonatomic, strong) NSTextField *modelField;
@property (nonatomic, strong) TSStyledButton *testApiBtn;
@property (nonatomic, strong) NSTextField *apiBadgeLabel;
@property (nonatomic, assign) BOOL showKey;

// Bottom bar
@property (nonatomic, strong) NSTextField *statusLabel;
@property (nonatomic, strong) TSStyledButton *saveBtn;
@property (nonatomic, strong) TSStyledButton *cancelBtn;
@end

@implementation TSSettingsWindow
- (void)switchToTab:(NSInteger)idx {
    self.activeTab = idx;
    for (NSUInteger i = 0; i < self.tabBtns.count; i++) {
        self.tabBtns[i].activeTab = ((NSInteger)i == idx);
        [self.tabBtns[i] setNeedsDisplay:YES];
        self.tabPages[i].hidden = ((NSInteger)i != idx);
    }
    NSArray *titles = @[
        @[@"通用偏好设置", @"配置截图保存路径、终端输出格式与开机行为"],
        @[@"快捷按键设置", @"分别管理截图与剪贴板翻译快捷键"],
        @[@"DeepSeek AI 智能识别", @"配置截图 OCR、截图翻译与剪贴板翻译所用的模型 API"],
        @[@"关于 TermShot", @"轻量、极速、无广告的现代化原生截图工具"],
    ];
    self.headerTitle.stringValue = titles[idx][0];
    self.headerSub.stringValue = titles[idx][1];
}

- (void)tabClicked:(TSSettingsTabButton *)sender {
    [self switchToTab:sender.tag];
}

- (void)onBrowse:(id)s { if (self.cbs.on_browse) self.cbs.on_browse(self.userData); }
- (void)onOpenDir:(id)s { if (self.cbs.on_open_dir) self.cbs.on_open_dir(self.userData); }
- (void)onOpenCfg:(id)s { if (self.cbs.on_open_config) self.cbs.on_open_config(self.userData); }
- (void)onOpenPerm:(id)s { if (self.cbs.on_open_perm) self.cbs.on_open_perm(self.userData); }
- (void)onRecCap:(id)s { if (self.cbs.on_record_hotkey) self.cbs.on_record_hotkey(0, self.userData); }
- (void)onResetCap:(id)s { if (self.cbs.on_reset_hotkey) self.cbs.on_reset_hotkey(0, self.userData); }
- (void)onRecTrans:(id)s { if (self.cbs.on_record_hotkey) self.cbs.on_record_hotkey(1, self.userData); }
- (void)onResetTrans:(id)s { if (self.cbs.on_reset_hotkey) self.cbs.on_reset_hotkey(1, self.userData); }
- (void)onTestApi:(id)s { if (self.cbs.on_test_api) self.cbs.on_test_api(self.userData); }
- (void)onSave:(id)s { if (self.cbs.on_save) self.cbs.on_save(self.userData); }
- (void)onCancel:(id)s { if (self.cbs.on_cancel) self.cbs.on_cancel(self.userData); }

- (void)onToggleKey:(id)s {
    self.showKey = !self.showKey;
    if (self.showKey) {
        self.apiPlainField.stringValue = self.apiSecureField.stringValue;
        self.apiSecureField.hidden = YES;
        self.apiPlainField.hidden = NO;
        [self makeFirstResponder:self.apiPlainField];
        self.toggleKeyBtn.title = @"隐藏";
    } else {
        self.apiSecureField.stringValue = self.apiPlainField.stringValue;
        self.apiPlainField.hidden = YES;
        self.apiSecureField.hidden = NO;
        [self makeFirstResponder:self.apiSecureField];
        self.toggleKeyBtn.title = @"显示";
    }
    [self.toggleKeyBtn setNeedsDisplay:YES];
}

- (BOOL)windowShouldClose:(NSWindow *)sender {
    if (self.cbs.on_cancel) {
        self.cbs.on_cancel(self.userData);
        return NO;
    }
    return YES;
}

- (void)windowWillClose:(NSNotification *)notification {
    if (self.cbs.on_close) {
        void (*fn)(void *) = self.cbs.on_close;
        _cbs.on_close = NULL;
        fn(self.userData);
    }
}

- (void)sendEvent:(NSEvent *)event {
    if (self.recordingHotkey && (event.type == NSEventTypeKeyDown || event.type == NSEventTypeFlagsChanged)) {
        if (event.type == NSEventTypeKeyDown) {
            uint32_t vk = mac_keycode_to_vk(event.keyCode);
            if (self.cbs.on_hotkey_input) {
                self.cbs.on_hotkey_input(vk, modifiers_from_ns(event.modifierFlags), self.userData);
            }
            return;
        } else {
            uint32_t mods = modifiers_from_ns(event.modifierFlags);
            if (mods != 0 && self.cbs.on_hotkey_input) {
                self.cbs.on_hotkey_input(0, mods, self.userData);
            }
        }
    } else if (!self.recordingHotkey && event.type == NSEventTypeKeyDown) {
        uint32_t vk = mac_keycode_to_vk(event.keyCode);
        if (vk == 0x1B) {
            [self onCancel:nil];
            return;
        }
        if (vk == 0x0D && ![self.firstResponder isKindOfClass:[NSTextView class]]) {
            [self onSave:nil];
            return;
        }
    }
    [super sendEvent:event];
}
@end

static NSTextField *make_dark_input(NSRect frame, NSString *val, BOOL readonly) {
    NSTextField *tf = [[NSTextField alloc] initWithFrame:frame];
    tf.stringValue = val ?: @"";
    tf.font = make_ui_font(12.5, NO);
    tf.textColor = ns_color_argb(0xFFE6EDF3);
    tf.backgroundColor = ns_color_argb(0xFF0D1117);
    tf.drawsBackground = YES;
    tf.editable = !readonly;
    tf.selectable = YES;
    tf.bezeled = YES;
    tf.bezelStyle = NSTextFieldSquareBezel;
    return tf;
}

static TSStyledButton *make_btn(NSRect frame, NSString *title, BOOL primary, id target, SEL sel) {
    TSStyledButton *b = [[TSStyledButton alloc] initWithFrame:frame];
    b.title = title;
    b.primary = primary;
    b.target = target;
    b.action = sel;
    return b;
}

static TSCardView *make_card(NSRect frame) {
    TSCardView *c = [[TSCardView alloc] initWithFrame:frame];
    c.cornerRadius = 8;
    c.bgColor = ns_color_argb(0xFF1A212B);
    c.borderColor = ns_color_argb(0xFF2A3340);
    return c;
}

void *ts_settings_create(const TSSettingsInit *init, const TSSettingsCallbacks *cbs, void *user_data) {
    ts_app_init();
    @autoreleasepool {
        CGFloat W = 680.0, H = 540.0;
        NSScreen *sc = [NSScreen mainScreen] ?: [NSScreen screens].firstObject;
        NSRect work = sc ? sc.visibleFrame : NSMakeRect(0, 0, 1440, 900);
        NSRect frame = NSMakeRect(work.origin.x + (work.size.width - W) * 0.5,
                                  work.origin.y + (work.size.height - H) * 0.5,
                                  W, H);
        NSWindowStyleMask mask = NSWindowStyleMaskTitled | NSWindowStyleMaskClosable;
        TSSettingsWindow *win = [[TSSettingsWindow alloc] initWithContentRect:frame
                                                                    styleMask:mask
                                                                      backing:NSBackingStoreBuffered
                                                                        defer:NO];
        win.releasedWhenClosed = NO;
        win.title = @"TermShot — 设置";
        win.delegate = win;
        win.userData = user_data;
        if (cbs) win.cbs = *cbs;
        if (@available(macOS 10.14, *)) {
            win.appearance = [NSAppearance appearanceNamed:NSAppearanceNameDarkAqua];
        }
        win.backgroundColor = ns_color_argb(0xFF12161C);

        NSView *root = win.contentView;
        root.wantsLayer = YES;
        root.layer.backgroundColor = ns_color_argb(0xFF12161C).CGColor;

        CGFloat sidebarW = 180.0;
        CGFloat bottomH = 56.0;

        // Sidebar
        NSView *sidebar = [[NSView alloc] initWithFrame:NSMakeRect(0, bottomH, sidebarW, H - bottomH)];
        sidebar.wantsLayer = YES;
        sidebar.layer.backgroundColor = ns_color_argb(0xFF161B24).CGColor;
        [root addSubview:sidebar];

        NSView *sdiv = [[NSView alloc] initWithFrame:NSMakeRect(sidebarW - 1, bottomH, 1, H - bottomH)];
        sdiv.wantsLayer = YES;
        sdiv.layer.backgroundColor = ns_color_argb(0xFF2A3340).CGColor;
        [root addSubview:sdiv];

        NSTextField *brand = make_label(@"TermShot", make_ui_font(17, YES), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
        brand.frame = NSMakeRect(16, (H - bottomH) - 42, sidebarW - 32, 24);
        [sidebar addSubview:brand];
        NSTextField *brandSub = make_label(@"v1.0.0 · 智能极速截图", make_ui_font(11.5, NO), ns_color_argb(0xFF8B94A0), NSTextAlignmentLeft);
        brandSub.frame = NSMakeRect(16, (H - bottomH) - 62, sidebarW - 32, 18);
        [sidebar addSubview:brandSub];

        NSArray *tabItems = @[
            @[@"01", @"通用偏好"],
            @[@"02", @"快捷按键"],
            @[@"03", @"AI 智能"],
            @[@"04", @"关于软件"],
        ];
        NSMutableArray *tBtns = [NSMutableArray array];
        for (NSUInteger i = 0; i < tabItems.count; i++) {
            CGFloat ty = (H - bottomH) - 116 - (CGFloat)i * 46.0;
            TSSettingsTabButton *tb = [[TSSettingsTabButton alloc] initWithFrame:NSMakeRect(10, ty, sidebarW - 20, 40)];
            tb.numStr = tabItems[i][0];
            tb.labelStr = tabItems[i][1];
            tb.tag = (NSInteger)i;
            tb.target = win;
            tb.action = @selector(tabClicked:);
            [sidebar addSubview:tb];
            [tBtns addObject:tb];
        }
        win.tabBtns = tBtns;

        // Content Header
        CGFloat contentX = 200.0;
        CGFloat cardW = 460.0;
        win.headerTitle = make_label(@"", make_ui_font(17, YES), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
        win.headerTitle.frame = NSMakeRect(contentX, H - 40, cardW, 24);
        win.headerSub = make_label(@"", make_ui_font(12, NO), ns_color_argb(0xFF8B94A0), NSTextAlignmentLeft);
        win.headerSub.frame = NSMakeRect(contentX, H - 60, cardW, 18);
        [root addSubview:win.headerTitle];
        [root addSubview:win.headerSub];

        // Pages container (coords: y from bottom of page = bottomH .. H - 64)
        CGFloat pageH = H - bottomH - 64.0;
        NSMutableArray *pages = [NSMutableArray array];
        for (int i = 0; i < 4; i++) {
            NSView *p = [[NSView alloc] initWithFrame:NSMakeRect(contentX, bottomH, cardW, pageH)];
            p.hidden = (i != 0);
            [root addSubview:p];
            [pages addObject:p];
        }
        win.tabPages = pages;

        // ================= Page 0: General =================
        {
            NSView *p0 = pages[0];
            TSCardView *c1 = make_card(NSMakeRect(0, pageH - 160, cardW, 154));
            [p0 addSubview:c1];
            NSTextField *c1t = make_label(@"存储路径与终端交互", make_ui_font(13.5, YES), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
            c1t.frame = NSMakeRect(16, 122, cardW - 32, 22);
            [c1 addSubview:c1t];
            NSTextField *dirLbl = make_label(@"保存目录:", make_ui_font(12.5, NO), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
            dirLbl.frame = NSMakeRect(16, 86, 68, 22);
            [c1 addSubview:dirLbl];

            NSString *dirStr = init->save_dir ? [NSString stringWithUTF8String:init->save_dir] : @"";
            win.dirField = make_dark_input(NSMakeRect(86, 84, 220, 26), dirStr, NO);
            [c1 addSubview:win.dirField];
            [c1 addSubview:make_btn(NSMakeRect(314, 84, 66, 26), @"浏览...", NO, win, @selector(onBrowse:))];
            [c1 addSubview:make_btn(NSMakeRect(386, 84, 60, 26), @"打开", NO, win, @selector(onOpenDir:))];

            win.quoteCheck = [NSButton checkboxWithTitle:@"复制保存路径时包含双引号 (方便终端直接 cd / cat 路径)"
                                                  target:nil action:nil];
            win.quoteCheck.frame = NSMakeRect(16, 48, cardW - 32, 22);
            win.quoteCheck.font = make_ui_font(12, NO);
            win.quoteCheck.state = init->quote_path ? NSControlStateValueOn : NSControlStateValueOff;
            [c1 addSubview:win.quoteCheck];

            NSTextField *dirNote = make_label(@"说明: 留空将默认使用 macOS 系统 ~/Pictures/Screenshots 目录",
                                              make_ui_font(11.5, NO), ns_color_argb(0xFF8B94A0), NSTextAlignmentLeft);
            dirNote.frame = NSMakeRect(16, 16, cardW - 32, 20);
            [c1 addSubview:dirNote];

            TSCardView *c2 = make_card(NSMakeRect(0, pageH - 314, cardW, 140));
            [p0 addSubview:c2];
            NSTextField *c2t = make_label(@"截图后行为与启动项", make_ui_font(13.5, YES), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
            c2t.frame = NSMakeRect(16, 108, cardW - 32, 22);
            [c2 addSubview:c2t];
            NSTextField *actLbl = make_label(@"完成时动作:", make_ui_font(12.5, NO), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
            actLbl.frame = NSMakeRect(16, 70, 80, 22);
            [c2 addSubview:actLbl];

            win.actionPopup = [[NSPopUpButton alloc] initWithFrame:NSMakeRect(100, 68, 344, 26) pullsDown:NO];
            win.actionPopup.font = make_ui_font(12.5, NO);
            [win.actionPopup addItemsWithTitles:@[
                @"每次询问（截完后选择）",
                @"保存图片",
                @"复制图片",
                @"复制保存图片的地址",
                @"贴到桌面",
                @"识别文字并显示结果",
                @"翻译并显示结果",
            ]];
            if (init->post_action_idx >= 0 && init->post_action_idx < 7) {
                [win.actionPopup selectItemAtIndex:init->post_action_idx];
            }
            [c2 addSubview:win.actionPopup];

            win.startupCheck = [NSButton checkboxWithTitle:@"开机时自动启动 TermShot (随 macOS 登录并在菜单栏常驻)"
                                                    target:nil action:nil];
            win.startupCheck.frame = NSMakeRect(16, 26, cardW - 32, 22);
            win.startupCheck.font = make_ui_font(12, NO);
            win.startupCheck.state = init->start_at_login ? NSControlStateValueOn : NSControlStateValueOff;
            [c2 addSubview:win.startupCheck];
        }

        // ================= Page 1: Hotkeys =================
        {
            NSView *p1 = pages[1];
            TSCardView *c1 = make_card(NSMakeRect(0, pageH - 160, cardW, 154));
            [p1 addSubview:c1];
            NSTextField *c1t = make_label(@"全局快捷键", make_ui_font(13.5, YES), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
            c1t.frame = NSMakeRect(16, 122, cardW - 32, 22);
            [c1 addSubview:c1t];

            NSTextField *l1 = make_label(@"截图快捷键:", make_ui_font(12.5, NO), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
            l1.frame = NSMakeRect(16, 86, 80, 22);
            [c1 addSubview:l1];
            NSString *hk1 = init->capture_hotkey_text ? [NSString stringWithUTF8String:init->capture_hotkey_text] : @"";
            win.hotkeyField = make_dark_input(NSMakeRect(100, 84, 174, 26), hk1, YES);
            [c1 addSubview:win.hotkeyField];
            win.recordBtn = make_btn(NSMakeRect(282, 84, 78, 26), @"录制", NO, win, @selector(onRecCap:));
            [c1 addSubview:win.recordBtn];
            [c1 addSubview:make_btn(NSMakeRect(366, 84, 78, 26), @"恢复默认", NO, win, @selector(onResetCap:))];

            NSTextField *l2 = make_label(@"翻译快捷键:", make_ui_font(12.5, NO), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
            l2.frame = NSMakeRect(16, 46, 80, 22);
            [c1 addSubview:l2];
            NSString *hk2 = init->translate_hotkey_text ? [NSString stringWithUTF8String:init->translate_hotkey_text] : @"";
            win.transHotkeyField = make_dark_input(NSMakeRect(100, 44, 174, 26), hk2, YES);
            [c1 addSubview:win.transHotkeyField];
            win.transRecordBtn = make_btn(NSMakeRect(282, 44, 78, 26), @"录制", NO, win, @selector(onRecTrans:));
            [c1 addSubview:win.transRecordBtn];
            [c1 addSubview:make_btn(NSMakeRect(366, 44, 78, 26), @"恢复默认", NO, win, @selector(onResetTrans:))];

            NSTextField *note = make_label(@"推荐组合键或 F1-F12 单键；两组快捷键不能相同，录制时按 Esc 可取消。",
                                           make_ui_font(11.5, NO), ns_color_argb(0xFF8B94A0), NSTextAlignmentLeft);
            note.frame = NSMakeRect(16, 14, cardW - 32, 20);
            [c1 addSubview:note];

            TSCardView *c2 = make_card(NSMakeRect(0, pageH - 404, cardW, 230));
            [p1 addSubview:c2];
            NSTextField *c2t = make_label(@"快捷键速查", make_ui_font(13.5, YES), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
            c2t.frame = NSMakeRect(16, 198, cardW - 32, 22);
            [c2 addSubview:c2t];

            NSArray *cheat = @[
                @[hk2, @"翻译剪贴板文字，并在置顶窗口显示完整译文"],
                @[@"Enter / 双击选区", @"完成截图并根据设定写入剪贴板或保存文件"],
                @[@"S / Esc", @"S 另存为 PNG，Esc 取消并退出当前截图"],
                @[@"C / T / P", @"C 复制图片，T 贴到桌面，P 复制文件路径"],
                @[@"O / L", @"O 识别文字 (OCR)，L 截图翻译"],
                @[@"Z / R", @"Z 撤销上一步标注，R 进入滚动长图截图"],
            ];
            for (NSUInteger i = 0; i < cheat.count; i++) {
                CGFloat ry = 166.0 - (CGFloat)i * 28.0;
                TSCardView *badge = [[TSCardView alloc] initWithFrame:NSMakeRect(16, ry, 126, 22)];
                badge.cornerRadius = 4;
                badge.bgColor = ns_color_argb(0xFF111620);
                badge.borderColor = ns_color_argb(0xFF2A3340);
                NSTextField *bl = make_label(cheat[i][0], make_mono_font(11, YES), ns_color_argb(0xFF5EF0C0), NSTextAlignmentCenter);
                bl.frame = NSMakeRect(2, 2, 122, 18);
                [badge addSubview:bl];
                [c2 addSubview:badge];

                NSTextField *dl = make_label(cheat[i][1], make_ui_font(12, NO), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
                dl.frame = NSMakeRect(154, ry + 2, cardW - 170, 18);
                [c2 addSubview:dl];
            }
        }

        // ================= Page 2: AI =================
        {
            NSView *p2 = pages[2];
            TSCardView *c1 = make_card(NSMakeRect(0, pageH - 172, cardW, 166));
            [p2 addSubview:c1];
            NSTextField *c1t = make_label(@"🔑   DeepSeek API 密钥与模型", make_ui_font(13.5, YES), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
            c1t.frame = NSMakeRect(16, 134, 240, 22);
            [c1 addSubview:c1t];

            NSString *keyStr = init->api_key ? [NSString stringWithUTF8String:init->api_key] : @"";
            BOOL hasKey = [[keyStr stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceAndNewlineCharacterSet]] length] > 0;
            win.apiBadgeLabel = make_label(hasKey ? @"●  API Key 已就绪" : @"○  未配置 (功能受限)",
                                           make_ui_font(11.5, NO),
                                           hasKey ? ns_color_argb(0xFF2DE2A8) : ns_color_argb(0xFFE5C07B),
                                           NSTextAlignmentRight);
            win.apiBadgeLabel.frame = NSMakeRect(cardW - 176, 134, 160, 22);
            [c1 addSubview:win.apiBadgeLabel];

            NSTextField *kl = make_label(@"API Key:", make_ui_font(12.5, NO), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
            kl.frame = NSMakeRect(16, 98, 66, 22);
            [c1 addSubview:kl];

            win.apiSecureField = [[NSSecureTextField alloc] initWithFrame:NSMakeRect(84, 96, 284, 26)];
            win.apiSecureField.stringValue = keyStr;
            win.apiSecureField.font = make_ui_font(12.5, NO);
            win.apiSecureField.textColor = ns_color_argb(0xFFE6EDF3);
            win.apiSecureField.backgroundColor = ns_color_argb(0xFF0D1117);
            win.apiSecureField.drawsBackground = YES;
            win.apiSecureField.bezeled = YES;
            [c1 addSubview:win.apiSecureField];

            win.apiPlainField = make_dark_input(NSMakeRect(84, 96, 284, 26), keyStr, NO);
            win.apiPlainField.hidden = YES;
            [c1 addSubview:win.apiPlainField];

            win.toggleKeyBtn = make_btn(NSMakeRect(374, 96, 70, 26), @"显示", NO, win, @selector(onToggleKey:));
            [c1 addSubview:win.toggleKeyBtn];

            NSTextField *ml = make_label(@"识别模型:", make_ui_font(12.5, NO), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
            ml.frame = NSMakeRect(16, 58, 66, 22);
            [c1 addSubview:ml];

            NSString *mStr = init->model ? [NSString stringWithUTF8String:init->model] : @"deepseek-flash";
            win.modelField = make_dark_input(NSMakeRect(84, 56, 254, 26), mStr, NO);
            [c1 addSubview:win.modelField];

            win.testApiBtn = make_btn(NSMakeRect(346, 56, 98, 26), @"验证连接", NO, win, @selector(onTestApi:));
            [c1 addSubview:win.testApiBtn];

            NSTextField *mnote = make_label(@"推荐模型: deepseek-chat 或 deepseek-flash",
                                            make_ui_font(11.5, NO), ns_color_argb(0xFF8B94A0), NSTextAlignmentLeft);
            mnote.frame = NSMakeRect(84, 20, cardW - 100, 20);
            [c1 addSubview:mnote];

            TSCardView *c2 = make_card(NSMakeRect(0, pageH - 404, cardW, 220));
            [p2 addSubview:c2];
            NSTextField *c2t = make_label(@"AI 智能视觉工作流", make_ui_font(13.5, YES), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
            c2t.frame = NSMakeRect(16, 186, cardW - 32, 22);
            [c2 addSubview:c2t];

            NSArray *feats = @[
                @[@"OCR 文字提取", @"截图完成后点击工具栏提取文字，自动保留多行排版、代码缩进及表格数据。"],
                @[@"多语言即时翻译", @"支持截图翻译与剪贴板文字翻译，中英自动互译并在独立窗口中呈现完整译文。"],
                @[@"本地按需与隐私安全", @"仅在主动触发“提取文字”或“翻译”时调用官方 API，不会在后台自动上传。"],
            ];
            for (NSUInteger i = 0; i < feats.count; i++) {
                CGFloat fy = 144.0 - (CGFloat)i * 58.0;
                NSTextField *ft = make_label(feats[i][0], make_ui_font(12.5, YES), ns_color_argb(0xFF5EF0C0), NSTextAlignmentLeft);
                ft.frame = NSMakeRect(16, fy + 18, cardW - 32, 20);
                [c2 addSubview:ft];
                NSTextField *fd = make_label(feats[i][1], make_ui_font(11.5, NO), ns_color_argb(0xFF8B94A0), NSTextAlignmentLeft);
                fd.lineBreakMode = NSLineBreakByWordWrapping;
                fd.maximumNumberOfLines = 2;
                fd.frame = NSMakeRect(16, fy - 12, cardW - 32, 32);
                [c2 addSubview:fd];
            }
        }

        // ================= Page 3: About =================
        {
            NSView *p3 = pages[3];
            TSCardView *c1 = make_card(NSMakeRect(0, pageH - 210, cardW, 204));
            [p3 addSubview:c1];
            NSTextField *c1t = make_label(@"💻   TermShot for macOS", make_ui_font(13.5, YES), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
            c1t.frame = NSMakeRect(16, 170, cardW - 32, 22);
            [c1 addSubview:c1t];
            NSTextField *ver = make_label(@"版本 1.0.0 · 专为开发者与极客打造的超轻量高性能截图利器",
                                          make_ui_font(12, NO), ns_color_argb(0xFF5EF0C0), NSTextAlignmentLeft);
            ver.frame = NSMakeRect(16, 146, cardW - 32, 20);
            [c1 addSubview:ver];

            NSArray *aboutItems = @[
                @"极速冷启动与低内存：纯 Rust + 原生 AppKit 实现，无 Electron 运行时",
                @"🎨 原生 Retina Quartz 渲染：ScreenCaptureKit 截图 + 高分屏无损缩放",
                @"🛡 永久开源免费：基于 MIT License 开源协议，支持个人与团队自由使用",
                @"开发者终端友好：复制文件路径可附带双引号，方便在 zsh / bash 中引用",
            ];
            for (NSUInteger i = 0; i < aboutItems.count; i++) {
                NSTextField *it = make_label(aboutItems[i], make_ui_font(12, NO), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
                it.frame = NSMakeRect(16, 114 - (CGFloat)i * 28.0, cardW - 32, 22);
                [c1 addSubview:it];
            }

            TSCardView *c2 = make_card(NSMakeRect(0, pageH - 366, cardW, 144));
            [p3 addSubview:c2];
            NSTextField *c2t = make_label(@"📂   系统权限与本地配置", make_ui_font(13.5, YES), ns_color_argb(0xFFE6EDF3), NSTextAlignmentLeft);
            c2t.frame = NSMakeRect(16, 112, cardW - 32, 22);
            [c2 addSubview:c2t];

            NSString *permStr = init->has_screen_perm
                ? @"屏幕录制权限:  ● 已授权，可正常截取屏幕与窗口"
                : @"屏幕录制权限:  ○ 未授权 (请点击右侧按钮在系统设置中开启)";
            NSTextField *permLbl = make_label(permStr, make_ui_font(12, NO),
                                              init->has_screen_perm ? ns_color_argb(0xFF38D996) : ns_color_argb(0xFFF5B942),
                                              NSTextAlignmentLeft);
            permLbl.frame = NSMakeRect(16, 78, 300, 22);
            [c2 addSubview:permLbl];
            [c2 addSubview:make_btn(NSMakeRect(320, 76, 124, 26), @"屏幕录制权限", NO, win, @selector(onOpenPerm:))];

            NSTextField *cfgLbl = make_label(@"配置路径: ~/Library/Application Support/TermShot/settings.json",
                                             make_ui_font(11.5, NO), ns_color_argb(0xFF8B94A0), NSTextAlignmentLeft);
            cfgLbl.frame = NSMakeRect(16, 30, 290, 22);
            [c2 addSubview:cfgLbl];
            [c2 addSubview:make_btn(NSMakeRect(320, 28, 124, 26), @"打开配置目录", NO, win, @selector(onOpenCfg:))];
        }

        // Bottom Bar
        NSView *bottom = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, W, bottomH)];
        bottom.wantsLayer = YES;
        bottom.layer.backgroundColor = ns_color_argb(0xFF141922).CGColor;
        [root addSubview:bottom];
        NSView *bdiv = [[NSView alloc] initWithFrame:NSMakeRect(0, bottomH - 1, W, 1)];
        bdiv.wantsLayer = YES;
        bdiv.layer.backgroundColor = ns_color_argb(0xFF2A3340).CGColor;
        [bottom addSubview:bdiv];

        win.statusLabel = make_label(@"提示: Esc 键关闭窗口，Enter 键保存配置",
                                     make_ui_font(11.5, NO), ns_color_argb(0xFF8B94A0), NSTextAlignmentLeft);
        win.statusLabel.frame = NSMakeRect(18, 16, 390, 22);
        [bottom addSubview:win.statusLabel];

        win.saveBtn = make_btn(NSMakeRect(424, 12, 126, 32), @"保存设置 (Enter)", YES, win, @selector(onSave:));
        win.cancelBtn = make_btn(NSMakeRect(558, 12, 102, 32), @"取消 (Esc)", NO, win, @selector(onCancel:));
        [bottom addSubview:win.saveBtn];
        [bottom addSubview:win.cancelBtn];

        [win switchToTab:0];
        [NSApp activateIgnoringOtherApps:YES];
        if (@available(macOS 14.0, *)) {
            [NSApp activate];
        }
        [win makeKeyAndOrderFront:nil];
        [win orderFrontRegardless];
        return (void *)CFBridgingRetain(win);
    }
}

void ts_settings_set_status(void *win_ptr, const char *status, int32_t kind) {
    if (!win_ptr || !status) return;
    @autoreleasepool {
        TSSettingsWindow *win = (__bridge TSSettingsWindow *)win_ptr;
        win.statusLabel.stringValue = [NSString stringWithUTF8String:status] ?: @"";
        if (kind == 1) win.statusLabel.textColor = ns_color_argb(0xFF38D996);
        else if (kind == 2) win.statusLabel.textColor = ns_color_argb(0xFFF07178);
        else if (kind == 3) win.statusLabel.textColor = ns_color_argb(0xFF5EF0C0);
        else win.statusLabel.textColor = ns_color_argb(0xFF8B94A0);
    }
}

void ts_settings_set_api_testing(void *win_ptr, bool testing, const char *status, int32_t kind) {
    if (!win_ptr) return;
    @autoreleasepool {
        TSSettingsWindow *win = (__bridge TSSettingsWindow *)win_ptr;
        win.testApiBtn.enabled = !testing;
        win.testApiBtn.title = testing ? @"验证中…" : @"验证连接";
        [win.testApiBtn setNeedsDisplay:YES];
        if (!testing && win.apiBadgeLabel) {
            if (kind == 1) {
                win.apiBadgeLabel.stringValue = @"●  API Key 已就绪";
                win.apiBadgeLabel.textColor = ns_color_argb(0xFF2DE2A8);
            } else if (kind == 2) {
                win.apiBadgeLabel.stringValue = @"●  验证未通过";
                win.apiBadgeLabel.textColor = ns_color_argb(0xFFF07178);
            }
        }
        if (status) ts_settings_set_status(win_ptr, status, kind);
        [win displayIfNeeded];
    }
}

void ts_settings_set_hotkey_display(void *win_ptr, int32_t target, const char *text, bool recording, bool any_recording) {
    if (!win_ptr) return;
    @autoreleasepool {
        TSSettingsWindow *win = (__bridge TSSettingsWindow *)win_ptr;
        win.recordingHotkey = any_recording;
        NSTextField *field = (target == 0) ? win.hotkeyField : win.transHotkeyField;
        TSStyledButton *btn = (target == 0) ? win.recordBtn : win.transRecordBtn;
        if (text) field.stringValue = [NSString stringWithUTF8String:text] ?: @"";
        btn.danger = recording;
        btn.title = recording ? @"停止录制" : @"录制";
        [btn setNeedsDisplay:YES];
    }
}

void ts_settings_set_dir(void *win_ptr, const char *path) {
    if (!win_ptr || !path) return;
    @autoreleasepool {
        TSSettingsWindow *win = (__bridge TSSettingsWindow *)win_ptr;
        win.dirField.stringValue = [NSString stringWithUTF8String:path] ?: @"";
    }
}

void ts_settings_get_values(void *win_ptr, TSSettingsValues *out) {
    if (!win_ptr || !out) return;
    memset(out, 0, sizeof(*out));
    @autoreleasepool {
        TSSettingsWindow *win = (__bridge TSSettingsWindow *)win_ptr;
        const char *d = [win.dirField.stringValue UTF8String];
        out->save_dir = strdup(d ?: "");
        out->quote_path = (win.quoteCheck.state == NSControlStateValueOn);
        out->post_action_idx = (int32_t)[win.actionPopup indexOfSelectedItem];
        out->start_at_login = (win.startupCheck.state == NSControlStateValueOn);
        NSString *key = win.showKey ? win.apiPlainField.stringValue : win.apiSecureField.stringValue;
        out->api_key = strdup([key UTF8String] ?: "");
        out->model = strdup([win.modelField.stringValue UTF8String] ?: "");
    }
}

void ts_settings_free_values(TSSettingsValues *v) {
    if (!v) return;
    if (v->save_dir) free(v->save_dir);
    if (v->api_key) free(v->api_key);
    if (v->model) free(v->model);
    memset(v, 0, sizeof(*v));
}

void ts_settings_close(void *win_ptr) {
    if (!win_ptr) return;
    @autoreleasepool {
        TSSettingsWindow *win = (TSSettingsWindow *)CFBridgingRelease(win_ptr);
        TSSettingsCallbacks tmp = win.cbs;
        tmp.on_close = NULL;
        win.cbs = tmp;
        [win close];
    }
}
