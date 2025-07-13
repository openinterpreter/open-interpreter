"""
Enhanced Window Manager for Open Interpreter
Manages application windows, switching, and detection
"""

import subprocess
import platform
import time
import re
from typing import List, Dict, Optional, Tuple
import psutil


class WindowManager:
    """
    Advanced window management for cross-platform operation
    Handles window detection, switching, and control
    """
    
    def __init__(self, computer):
        self.computer = computer
        self.os_type = platform.system().lower()
        self.cached_windows = []
        self.last_cache_time = 0
        self.cache_duration = 2  # seconds
        
    def get_all_windows(self, force_refresh: bool = False) -> List[Dict]:
        """
        Get all open windows with detailed information
        Uses caching to avoid excessive system calls
        """
        current_time = time.time()
        
        if (not force_refresh and 
            self.cached_windows and 
            current_time - self.last_cache_time < self.cache_duration):
            return self.cached_windows
        
        windows = []
        
        try:
            if self.os_type == 'darwin':
                windows = self._get_macos_windows()
            elif self.os_type == 'linux':
                windows = self._get_linux_windows()
            elif self.os_type == 'windows':
                windows = self._get_windows_windows()
                
            self.cached_windows = windows
            self.last_cache_time = current_time
            
        except Exception as e:
            print(f"Error getting windows: {e}")
            
        return windows
    
    def _get_macos_windows(self) -> List[Dict]:
        """Get windows on macOS using AppleScript"""
        windows = []
        
        try:
            # Get all visible applications
            script = '''
            tell application "System Events"
                set appList to {}
                repeat with theApp in (every application process whose visible is true)
                    try
                        set appName to name of theApp
                        set windowList to {}
                        repeat with theWindow in (every window of theApp)
                            try
                                set windowTitle to title of theWindow
                                set windowList to windowList & {windowTitle}
                            end try
                        end repeat
                        set appList to appList & {{appName, windowList}}
                    end try
                end repeat
                return appList
            end tell
            '''
            
            result = subprocess.run(['osascript', '-e', script], 
                                  capture_output=True, text=True, timeout=10)
            
            if result.returncode == 0:
                # Parse the AppleScript output
                output = result.stdout.strip()
                if output:
                    windows = self._parse_macos_output(output)
                    
        except Exception as e:
            print(f"macOS window detection error: {e}")
            
        return windows
    
    def _parse_macos_output(self, output: str) -> List[Dict]:
        """Parse macOS AppleScript output"""
        windows = []
        
        try:
            # This is a simplified parser - AppleScript output can be complex
            lines = output.split('\n')
            for line in lines:
                if line.strip():
                    # Extract app and window info
                    # This would need more sophisticated parsing in practice
                    windows.append({
                        'id': len(windows),
                        'title': line.strip(),
                        'application': 'Unknown',
                        'platform': 'darwin'
                    })
        except Exception as e:
            print(f"Error parsing macOS output: {e}")
            
        return windows
    
    def _get_linux_windows(self) -> List[Dict]:
        """Get windows on Linux using wmctrl and xdotool"""
        windows = []
        
        try:
            # Try wmctrl first
            if self._command_exists('wmctrl'):
                result = subprocess.run(['wmctrl', '-l'], 
                                      capture_output=True, text=True, timeout=5)
                if result.returncode == 0:
                    for line in result.stdout.strip().split('\n'):
                        if line:
                            parts = line.split(None, 3)
                            if len(parts) >= 4:
                                windows.append({
                                    'id': parts[0],
                                    'desktop': parts[1],
                                    'application': parts[2],
                                    'title': parts[3],
                                    'platform': 'linux'
                                })
            
            # Try xdotool as backup
            elif self._command_exists('xdotool'):
                result = subprocess.run(['xdotool', 'search', '--name', '.*'], 
                                      capture_output=True, text=True, timeout=5)
                if result.returncode == 0:
                    window_ids = result.stdout.strip().split('\n')
                    for wid in window_ids:
                        if wid:
                            try:
                                name_result = subprocess.run(['xdotool', 'getwindowname', wid],
                                                           capture_output=True, text=True, timeout=2)
                                if name_result.returncode == 0:
                                    windows.append({
                                        'id': wid,
                                        'title': name_result.stdout.strip(),
                                        'application': 'Unknown',
                                        'platform': 'linux'
                                    })
                            except:
                                continue
                                
        except Exception as e:
            print(f"Linux window detection error: {e}")
            
        return windows
    
    def _get_windows_windows(self) -> List[Dict]:
        """Get windows on Windows using PowerShell"""
        windows = []
        
        try:
            ps_script = '''
            Get-Process | Where-Object {$_.MainWindowTitle -ne ""} | 
            Select-Object Id, ProcessName, MainWindowTitle, @{Name="WindowHandle";Expression={$_.MainWindowHandle}} |
            ConvertTo-Json
            '''
            
            result = subprocess.run(['powershell', '-Command', ps_script],
                                  capture_output=True, text=True, timeout=10)
            
            if result.returncode == 0:
                import json
                try:
                    data = json.loads(result.stdout)
                    if isinstance(data, list):
                        for item in data:
                            windows.append({
                                'id': str(item.get('Id', '')),
                                'handle': str(item.get('WindowHandle', '')),
                                'application': item.get('ProcessName', ''),
                                'title': item.get('MainWindowTitle', ''),
                                'platform': 'windows'
                            })
                    elif isinstance(data, dict):
                        windows.append({
                            'id': str(data.get('Id', '')),
                            'handle': str(data.get('WindowHandle', '')),
                            'application': data.get('ProcessName', ''),
                            'title': data.get('MainWindowTitle', ''),
                            'platform': 'windows'
                        })
                except json.JSONDecodeError:
                    # Fallback to text parsing
                    lines = result.stdout.strip().split('\n')
                    for line in lines[3:]:  # Skip headers
                        if line.strip():
                            parts = line.strip().split(None, 2)
                            if len(parts) >= 3:
                                windows.append({
                                    'id': parts[0],
                                    'application': parts[1],
                                    'title': parts[2],
                                    'platform': 'windows'
                                })
                                
        except Exception as e:
            print(f"Windows window detection error: {e}")
            
        return windows
    
    def find_window_by_title(self, title_pattern: str) -> Optional[Dict]:
        """Find a window by title pattern (regex supported)"""
        windows = self.get_all_windows()
        
        for window in windows:
            if re.search(title_pattern, window.get('title', ''), re.IGNORECASE):
                return window
                
        return None
    
    def find_windows_by_application(self, app_name: str) -> List[Dict]:
        """Find all windows belonging to an application"""
        windows = self.get_all_windows()
        
        matching_windows = []
        for window in windows:
            if app_name.lower() in window.get('application', '').lower():
                matching_windows.append(window)
                
        return matching_windows
    
    def switch_to_window(self, window: Dict) -> bool:
        """Switch to a specific window"""
        try:
            if self.os_type == 'darwin':
                return self._switch_macos_window(window)
            elif self.os_type == 'linux':
                return self._switch_linux_window(window)
            elif self.os_type == 'windows':
                return self._switch_windows_window(window)
        except Exception as e:
            print(f"Error switching to window: {e}")
            
        return False
    
    def _switch_macos_window(self, window: Dict) -> bool:
        """Switch to window on macOS"""
        try:
            app_name = window.get('application', '')
            window_title = window.get('title', '')
            
            if app_name:
                script = f'''
                tell application "{app_name}"
                    activate
                end tell
                '''
                result = subprocess.run(['osascript', '-e', script], 
                                      capture_output=True, timeout=5)
                return result.returncode == 0
                
        except Exception as e:
            print(f"macOS window switch error: {e}")
            
        return False
    
    def _switch_linux_window(self, window: Dict) -> bool:
        """Switch to window on Linux"""
        try:
            window_id = window.get('id', '')
            
            if window_id and self._command_exists('wmctrl'):
                result = subprocess.run(['wmctrl', '-i', '-a', window_id], 
                                      capture_output=True, timeout=5)
                return result.returncode == 0
                
            elif window_id and self._command_exists('xdotool'):
                result = subprocess.run(['xdotool', 'windowactivate', window_id], 
                                      capture_output=True, timeout=5)
                return result.returncode == 0
                
        except Exception as e:
            print(f"Linux window switch error: {e}")
            
        return False
    
    def _switch_windows_window(self, window: Dict) -> bool:
        """Switch to window on Windows"""
        try:
            window_handle = window.get('handle', '')
            
            if window_handle:
                ps_script = f'''
                Add-Type -TypeDefinition @"
                    using System;
                    using System.Runtime.InteropServices;
                    public class Win32 {{
                        [DllImport("user32.dll")]
                        public static extern bool SetForegroundWindow(IntPtr hWnd);
                        [DllImport("user32.dll")]
                        public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
                    }}
                "@
                [Win32]::ShowWindow({window_handle}, 9)
                [Win32]::SetForegroundWindow({window_handle})
                '''
                
                result = subprocess.run(['powershell', '-Command', ps_script],
                                      capture_output=True, timeout=5)
                return result.returncode == 0
                
        except Exception as e:
            print(f"Windows window switch error: {e}")
            
        return False
    
    def get_active_window(self) -> Optional[Dict]:
        """Get the currently active window"""
        try:
            if self.os_type == 'darwin':
                script = '''
                tell application "System Events"
                    set frontApp to name of first application process whose frontmost is true
                    tell process frontApp
                        set frontWindow to title of front window
                    end tell
                    return frontApp & "|" & frontWindow
                end tell
                '''
                result = subprocess.run(['osascript', '-e', script], 
                                      capture_output=True, text=True, timeout=5)
                if result.returncode == 0:
                    parts = result.stdout.strip().split('|')
                    if len(parts) >= 2:
                        return {
                            'application': parts[0],
                            'title': parts[1],
                            'platform': 'darwin'
                        }
                        
            elif self.os_type == 'linux' and self._command_exists('xdotool'):
                result = subprocess.run(['xdotool', 'getactivewindow'], 
                                      capture_output=True, text=True, timeout=5)
                if result.returncode == 0:
                    window_id = result.stdout.strip()
                    name_result = subprocess.run(['xdotool', 'getwindowname', window_id],
                                               capture_output=True, text=True, timeout=2)
                    if name_result.returncode == 0:
                        return {
                            'id': window_id,
                            'title': name_result.stdout.strip(),
                            'platform': 'linux'
                        }
                        
            elif self.os_type == 'windows':
                ps_script = '''
                Add-Type -TypeDefinition @"
                    using System;
                    using System.Runtime.InteropServices;
                    using System.Text;
                    public class Win32 {
                        [DllImport("user32.dll")]
                        public static extern IntPtr GetForegroundWindow();
                        [DllImport("user32.dll")]
                        public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
                    }
                "@
                $handle = [Win32]::GetForegroundWindow()
                $title = New-Object System.Text.StringBuilder 256
                [Win32]::GetWindowText($handle, $title, 256)
                Write-Output "$handle|$($title.ToString())"
                '''
                
                result = subprocess.run(['powershell', '-Command', ps_script],
                                      capture_output=True, text=True, timeout=5)
                if result.returncode == 0:
                    parts = result.stdout.strip().split('|')
                    if len(parts) >= 2:
                        return {
                            'handle': parts[0],
                            'title': parts[1],
                            'platform': 'windows'
                        }
                        
        except Exception as e:
            print(f"Error getting active window: {e}")
            
        return None
    
    def _command_exists(self, command: str) -> bool:
        """Check if a command exists on the system"""
        try:
            subprocess.run(['which', command], check=True, 
                         capture_output=True, timeout=2)
            return True
        except (subprocess.CalledProcessError, FileNotFoundError):
            return False
    
    def close_window(self, window: Dict) -> bool:
        """Close a specific window"""
        try:
            if self.os_type == 'darwin':
                app_name = window.get('application', '')
                script = f'''
                tell application "{app_name}"
                    close front window
                end tell
                '''
                result = subprocess.run(['osascript', '-e', script], 
                                      capture_output=True, timeout=5)
                return result.returncode == 0
                
            elif self.os_type == 'linux':
                window_id = window.get('id', '')
                if window_id and self._command_exists('wmctrl'):
                    result = subprocess.run(['wmctrl', '-i', '-c', window_id], 
                                          capture_output=True, timeout=5)
                    return result.returncode == 0
                    
            elif self.os_type == 'windows':
                window_handle = window.get('handle', '')
                if window_handle:
                    ps_script = f'''
                    Add-Type -TypeDefinition @"
                        using System;
                        using System.Runtime.InteropServices;
                        public class Win32 {{
                            [DllImport("user32.dll")]
                            public static extern bool CloseWindow(IntPtr hWnd);
                        }}
                    "@
                    [Win32]::CloseWindow({window_handle})
                    '''
                    result = subprocess.run(['powershell', '-Command', ps_script],
                                          capture_output=True, timeout=5)
                    return result.returncode == 0
                    
        except Exception as e:
            print(f"Error closing window: {e}")
            
        return False
    
    def get_window_summary(self) -> str:
        """Get a human-readable summary of open windows"""
        windows = self.get_all_windows()
        
        if not windows:
            return "No windows detected."
        
        summary = f"Found {len(windows)} open windows:\n"
        
        # Group by application
        apps = {}
        for window in windows:
            app = window.get('application', 'Unknown')
            if app not in apps:
                apps[app] = []
            apps[app].append(window.get('title', 'Untitled'))
        
        for app, titles in apps.items():
            summary += f"\n{app}:\n"
            for title in titles[:3]:  # Limit to 3 windows per app
                summary += f"  - {title}\n"
            if len(titles) > 3:
                summary += f"  ... and {len(titles) - 3} more\n"
        
        return summary