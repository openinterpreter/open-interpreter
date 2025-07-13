"""
Visible Terminal Manager for Open Interpreter Enhanced
Creates and manages a visible terminal window that users can see
"""

import subprocess
import platform
import time
import os
import tempfile
import threading
from typing import Optional, Dict, List


class VisibleTerminal:
    """
    Manages a visible terminal window for Open Interpreter operations
    Users can see what commands are being executed in real-time
    """
    
    def __init__(self, computer):
        self.computer = computer
        self.os_type = platform.system().lower()
        self.terminal_process = None
        self.terminal_pid = None
        self.command_history = []
        self.is_active = False
        
        # Terminal preferences by OS
        self.terminal_commands = self._get_terminal_commands()
        
    def _get_terminal_commands(self) -> Dict[str, List[str]]:
        """Get terminal commands for different operating systems"""
        if self.os_type == 'darwin':  # macOS
            return {
                'default': ['osascript', '-e', 'tell application "Terminal" to do script ""'],
                'iterm': ['osascript', '-e', 'tell application "iTerm" to create window with default profile'],
                'kitty': ['kitty'],
                'alacritty': ['alacritty']
            }
        elif self.os_type == 'linux':
            return {
                'gnome-terminal': ['gnome-terminal'],
                'konsole': ['konsole'],
                'xterm': ['xterm'],
                'kitty': ['kitty'],
                'alacritty': ['alacritty'],
                'terminator': ['terminator']
            }
        elif self.os_type == 'windows':
            return {
                'cmd': ['cmd', '/k'],
                'powershell': ['powershell'],
                'wt': ['wt']  # Windows Terminal
            }
        else:
            return {'default': ['xterm']}
    
    def open_terminal(self, title: str = "Open Interpreter Enhanced") -> bool:
        """
        Open a new visible terminal window
        Returns True if successful, False otherwise
        """
        try:
            if self.os_type == 'darwin':
                return self._open_macos_terminal(title)
            elif self.os_type == 'linux':
                return self._open_linux_terminal(title)
            elif self.os_type == 'windows':
                return self._open_windows_terminal(title)
            else:
                return False
        except Exception as e:
            print(f"Error opening terminal: {e}")
            return False
    
    def _open_macos_terminal(self, title: str) -> bool:
        """Open terminal on macOS"""
        try:
            # Try iTerm first, then Terminal
            iterm_script = f'''
            tell application "iTerm"
                create window with default profile
                tell current session of current window
                    write text "echo 'Open Interpreter Enhanced Terminal'"
                    write text "echo 'Commands executed by the AI will appear here'"
                    write text "echo '============================================'"
                end tell
            end tell
            '''
            
            terminal_script = f'''
            tell application "Terminal"
                do script "echo 'Open Interpreter Enhanced Terminal'; echo 'Commands executed by the AI will appear here'; echo '============================================'"
                set custom title of front window to "{title}"
            end tell
            '''
            
            # Try iTerm first
            try:
                result = subprocess.run(['osascript', '-e', iterm_script], 
                                      capture_output=True, timeout=10)
                if result.returncode == 0:
                    self.is_active = True
                    return True
            except:
                pass
            
            # Fallback to Terminal
            result = subprocess.run(['osascript', '-e', terminal_script], 
                                  capture_output=True, timeout=10)
            if result.returncode == 0:
                self.is_active = True
                return True
                
        except Exception as e:
            print(f"macOS terminal error: {e}")
            
        return False
    
    def _open_linux_terminal(self, title: str) -> bool:
        """Open terminal on Linux"""
        terminals_to_try = [
            ['gnome-terminal', '--title', title],
            ['konsole', '--title', title],
            ['xfce4-terminal', '--title', title],
            ['kitty', '--title', title],
            ['alacritty', '--title', title],
            ['xterm', '-title', title]
        ]
        
        for terminal_cmd in terminals_to_try:
            try:
                # Check if terminal exists
                subprocess.run(['which', terminal_cmd[0]], 
                             check=True, capture_output=True)
                
                # Open terminal
                self.terminal_process = subprocess.Popen(
                    terminal_cmd,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE
                )
                
                time.sleep(1)  # Give it time to open
                
                if self.terminal_process.poll() is None:  # Still running
                    self.terminal_pid = self.terminal_process.pid
                    self.is_active = True
                    
                    # Send welcome message
                    self._send_welcome_message()
                    return True
                    
            except (subprocess.CalledProcessError, FileNotFoundError):
                continue
                
        return False
    
    def _open_windows_terminal(self, title: str) -> bool:
        """Open terminal on Windows"""
        terminals_to_try = [
            ['wt', '-p', 'Command Prompt', '--title', title],
            ['powershell', '-NoExit', '-Command', f'$Host.UI.RawUI.WindowTitle = "{title}"'],
            ['cmd', '/k', f'title {title}']
        ]
        
        for terminal_cmd in terminals_to_try:
            try:
                self.terminal_process = subprocess.Popen(
                    terminal_cmd,
                    creationflags=subprocess.CREATE_NEW_CONSOLE
                )
                
                time.sleep(1)
                
                if self.terminal_process.poll() is None:
                    self.terminal_pid = self.terminal_process.pid
                    self.is_active = True
                    return True
                    
            except FileNotFoundError:
                continue
                
        return False
    
    def _send_welcome_message(self):
        """Send welcome message to the terminal"""
        welcome_commands = [
            'echo "Open Interpreter Enhanced Terminal"',
            'echo "Commands executed by the AI will appear here"',
            'echo "============================================"'
        ]
        
        for cmd in welcome_commands:
            self.execute_visible_command(cmd, show_in_terminal=False)
    
    def execute_visible_command(self, command: str, show_in_terminal: bool = True) -> Dict:
        """
        Execute a command in the visible terminal
        Returns the result and shows it to the user
        """
        if not self.is_active:
            if not self.open_terminal():
                # Fallback to regular execution
                return self.computer.terminal.run('shell', command)
        
        # Add to command history
        self.command_history.append({
            'command': command,
            'timestamp': time.time(),
            'visible': show_in_terminal
        })
        
        try:
            if show_in_terminal:
                # Show the command being executed
                self._display_command_in_terminal(command)
            
            # Execute the command and capture output
            if self.os_type == 'darwin':
                return self._execute_macos_command(command)
            elif self.os_type == 'linux':
                return self._execute_linux_command(command)
            elif self.os_type == 'windows':
                return self._execute_windows_command(command)
            else:
                # Fallback to regular execution
                return self.computer.terminal.run('shell', command)
                
        except Exception as e:
            error_msg = f"Error executing command: {e}"
            return {
                'type': 'console',
                'format': 'output',
                'content': error_msg
            }
    
    def _display_command_in_terminal(self, command: str):
        """Display the command that's about to be executed"""
        display_cmd = f'echo ">>> {command}"'
        
        if self.os_type == 'darwin':
            script = f'''
            tell application "Terminal"
                do script "{display_cmd}" in front window
            end tell
            '''
            subprocess.run(['osascript', '-e', script], capture_output=True)
            
        elif self.os_type == 'linux':
            # For Linux, we'll use a different approach since we can't easily send to existing terminal
            pass
            
        elif self.os_type == 'windows':
            # For Windows, similar challenge
            pass
    
    def _execute_macos_command(self, command: str) -> Dict:
        """Execute command on macOS and show in terminal"""
        try:
            # Execute in the visible terminal
            script = f'''
            tell application "Terminal"
                do script "{command}" in front window
            end tell
            '''
            subprocess.run(['osascript', '-e', script], capture_output=True)
            
            # Also execute normally to get the output
            result = subprocess.run(command, shell=True, capture_output=True, 
                                  text=True, timeout=30)
            
            return {
                'type': 'console',
                'format': 'output',
                'content': result.stdout + result.stderr
            }
            
        except Exception as e:
            return {
                'type': 'console',
                'format': 'output',
                'content': f"Error: {e}"
            }
    
    def _execute_linux_command(self, command: str) -> Dict:
        """Execute command on Linux"""
        try:
            # Execute the command
            result = subprocess.run(command, shell=True, capture_output=True, 
                                  text=True, timeout=30)
            
            return {
                'type': 'console',
                'format': 'output',
                'content': result.stdout + result.stderr
            }
            
        except Exception as e:
            return {
                'type': 'console',
                'format': 'output',
                'content': f"Error: {e}"
            }
    
    def _execute_windows_command(self, command: str) -> Dict:
        """Execute command on Windows"""
        try:
            # Execute the command
            result = subprocess.run(command, shell=True, capture_output=True, 
                                  text=True, timeout=30)
            
            return {
                'type': 'console',
                'format': 'output',
                'content': result.stdout + result.stderr
            }
            
        except Exception as e:
            return {
                'type': 'console',
                'format': 'output',
                'content': f"Error: {e}"
            }
    
    def close_terminal(self):
        """Close the visible terminal"""
        try:
            if self.terminal_process and self.terminal_process.poll() is None:
                self.terminal_process.terminate()
                time.sleep(1)
                if self.terminal_process.poll() is None:
                    self.terminal_process.kill()
                    
            self.is_active = False
            self.terminal_process = None
            self.terminal_pid = None
            
        except Exception as e:
            print(f"Error closing terminal: {e}")
    
    def is_terminal_active(self) -> bool:
        """Check if the terminal is still active"""
        if not self.is_active:
            return False
            
        if self.terminal_process:
            return self.terminal_process.poll() is None
            
        return True
    
    def get_command_history(self) -> List[Dict]:
        """Get the history of executed commands"""
        return self.command_history.copy()
    
    def clear_history(self):
        """Clear the command history"""
        self.command_history.clear()