"""
Enhanced Action Planner for Open Interpreter
Prioritizes terminal commands, then GUI interactions, then code execution
"""

import re
import subprocess
import time
from typing import Dict, List, Tuple, Optional
import psutil
import platform


class ActionPlanner:
    """
    Intelligent action planner that decides the best way to execute user requests.
    Priority order:
    1. Terminal commands (fastest, most reliable)
    2. GUI interactions (mouse/keyboard)
    3. Code execution (last resort)
    """
    
    def __init__(self, computer):
        self.computer = computer
        self.os_type = platform.system().lower()
        self.terminal_capabilities = self._detect_terminal_capabilities()
        self.gui_capabilities = self._detect_gui_capabilities()
        
    def _detect_terminal_capabilities(self) -> Dict[str, bool]:
        """Detect what can be done via terminal commands"""
        capabilities = {
            'file_operations': True,
            'network_operations': True,
            'process_management': True,
            'system_info': True,
            'package_management': True,
            'text_processing': True,
        }
        
        if self.os_type == 'darwin':  # macOS
            capabilities.update({
                'app_control': True,  # osascript
                'window_management': True,
                'notification': True,
            })
        elif self.os_type == 'linux':
            capabilities.update({
                'app_control': self._check_command('wmctrl') or self._check_command('xdotool'),
                'window_management': self._check_command('wmctrl'),
                'notification': self._check_command('notify-send'),
            })
        elif self.os_type == 'windows':
            capabilities.update({
                'app_control': True,  # PowerShell
                'window_management': True,
                'notification': True,
            })
            
        return capabilities
    
    def _detect_gui_capabilities(self) -> Dict[str, bool]:
        """Detect GUI automation capabilities"""
        return {
            'mouse_control': True,
            'keyboard_control': True,
            'screen_capture': True,
            'window_detection': True,
        }
    
    def _check_command(self, command: str) -> bool:
        """Check if a command is available"""
        try:
            subprocess.run([command, '--version'], 
                         capture_output=True, timeout=2)
            return True
        except (subprocess.TimeoutExpired, FileNotFoundError):
            return False
    
    def plan_action(self, user_request: str) -> Dict:
        """
        Analyze user request and create an execution plan
        Returns a plan with prioritized actions
        """
        request_lower = user_request.lower()
        
        # Analyze the request type
        action_type = self._classify_request(request_lower)
        
        # Create execution plan based on request type
        plan = {
            'primary_method': None,
            'fallback_methods': [],
            'actions': [],
            'requires_gui': False,
            'requires_terminal': False,
            'estimated_complexity': 'low'
        }
        
        if action_type == 'file_operation':
            plan = self._plan_file_operation(request_lower, user_request)
        elif action_type == 'app_control':
            plan = self._plan_app_control(request_lower, user_request)
        elif action_type == 'web_browsing':
            plan = self._plan_web_browsing(request_lower, user_request)
        elif action_type == 'system_info':
            plan = self._plan_system_info(request_lower, user_request)
        elif action_type == 'text_processing':
            plan = self._plan_text_processing(request_lower, user_request)
        else:
            plan = self._plan_general_task(request_lower, user_request)
            
        return plan
    
    def _classify_request(self, request: str) -> str:
        """Classify the type of request"""
        
        file_keywords = ['file', 'folder', 'directory', 'copy', 'move', 'delete', 'create', 'download']
        app_keywords = ['open', 'close', 'switch', 'application', 'program', 'window']
        web_keywords = ['browser', 'website', 'url', 'google', 'search', 'navigate']
        system_keywords = ['system', 'process', 'memory', 'cpu', 'disk', 'network']
        text_keywords = ['text', 'edit', 'write', 'read', 'find', 'replace']
        
        if any(keyword in request for keyword in file_keywords):
            return 'file_operation'
        elif any(keyword in request for keyword in app_keywords):
            return 'app_control'
        elif any(keyword in request for keyword in web_keywords):
            return 'web_browsing'
        elif any(keyword in request for keyword in system_keywords):
            return 'system_info'
        elif any(keyword in request for keyword in text_keywords):
            return 'text_processing'
        else:
            return 'general'
    
    def _plan_file_operation(self, request: str, original_request: str) -> Dict:
        """Plan file operations - prioritize terminal commands"""
        return {
            'primary_method': 'terminal',
            'fallback_methods': ['gui', 'code'],
            'actions': [
                {
                    'type': 'terminal_command',
                    'priority': 1,
                    'description': 'Use shell commands for file operations'
                },
                {
                    'type': 'gui_interaction',
                    'priority': 2,
                    'description': 'Use file manager if terminal fails'
                }
            ],
            'requires_terminal': True,
            'requires_gui': False,
            'estimated_complexity': 'low'
        }
    
    def _plan_app_control(self, request: str, original_request: str) -> Dict:
        """Plan application control - mix of terminal and GUI"""
        if self.os_type == 'darwin' and self.terminal_capabilities['app_control']:
            primary = 'terminal'  # osascript is very powerful on macOS
        elif self.os_type == 'linux' and self.terminal_capabilities['app_control']:
            primary = 'terminal'  # wmctrl/xdotool
        else:
            primary = 'gui'
            
        return {
            'primary_method': primary,
            'fallback_methods': ['gui', 'code'] if primary == 'terminal' else ['terminal', 'code'],
            'actions': [
                {
                    'type': 'app_detection',
                    'priority': 1,
                    'description': 'Detect currently running applications'
                },
                {
                    'type': 'window_management',
                    'priority': 2,
                    'description': 'Switch/control application windows'
                }
            ],
            'requires_terminal': primary == 'terminal',
            'requires_gui': primary == 'gui',
            'estimated_complexity': 'medium'
        }
    
    def _plan_web_browsing(self, request: str, original_request: str) -> Dict:
        """Plan web browsing - prioritize GUI with terminal support"""
        return {
            'primary_method': 'gui',
            'fallback_methods': ['terminal', 'code'],
            'actions': [
                {
                    'type': 'browser_detection',
                    'priority': 1,
                    'description': 'Detect open browsers and tabs'
                },
                {
                    'type': 'navigation',
                    'priority': 2,
                    'description': 'Navigate to URL or search'
                }
            ],
            'requires_terminal': False,
            'requires_gui': True,
            'estimated_complexity': 'medium'
        }
    
    def _plan_system_info(self, request: str, original_request: str) -> Dict:
        """Plan system information gathering - prioritize terminal"""
        return {
            'primary_method': 'terminal',
            'fallback_methods': ['code'],
            'actions': [
                {
                    'type': 'system_query',
                    'priority': 1,
                    'description': 'Use system commands to gather information'
                }
            ],
            'requires_terminal': True,
            'requires_gui': False,
            'estimated_complexity': 'low'
        }
    
    def _plan_text_processing(self, request: str, original_request: str) -> Dict:
        """Plan text processing - prioritize terminal tools"""
        return {
            'primary_method': 'terminal',
            'fallback_methods': ['gui', 'code'],
            'actions': [
                {
                    'type': 'text_command',
                    'priority': 1,
                    'description': 'Use command-line text tools'
                }
            ],
            'requires_terminal': True,
            'requires_gui': False,
            'estimated_complexity': 'low'
        }
    
    def _plan_general_task(self, request: str, original_request: str) -> Dict:
        """Plan general tasks - analyze and decide"""
        return {
            'primary_method': 'analysis',
            'fallback_methods': ['terminal', 'gui', 'code'],
            'actions': [
                {
                    'type': 'task_analysis',
                    'priority': 1,
                    'description': 'Analyze task and break it down'
                }
            ],
            'requires_terminal': False,
            'requires_gui': False,
            'estimated_complexity': 'high'
        }
    
    def get_current_applications(self) -> List[Dict]:
        """Get list of currently running applications"""
        apps = []
        
        try:
            for proc in psutil.process_iter(['pid', 'name', 'status']):
                try:
                    proc_info = proc.info
                    if proc_info['status'] == psutil.STATUS_RUNNING:
                        apps.append({
                            'pid': proc_info['pid'],
                            'name': proc_info['name'],
                            'status': proc_info['status']
                        })
                except (psutil.NoSuchProcess, psutil.AccessDenied):
                    continue
        except Exception as e:
            print(f"Error getting applications: {e}")
            
        return apps
    
    def get_window_list(self) -> List[Dict]:
        """Get list of open windows"""
        windows = []
        
        try:
            if self.os_type == 'darwin':
                # macOS - use osascript
                result = subprocess.run([
                    'osascript', '-e',
                    'tell application "System Events" to get name of every application process whose visible is true'
                ], capture_output=True, text=True, timeout=5)
                
                if result.returncode == 0:
                    app_names = result.stdout.strip().split(', ')
                    for i, name in enumerate(app_names):
                        windows.append({
                            'id': i,
                            'title': name.strip(),
                            'application': name.strip()
                        })
                        
            elif self.os_type == 'linux':
                # Linux - use wmctrl if available
                if self._check_command('wmctrl'):
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
                                        'title': parts[3]
                                    })
                                    
            elif self.os_type == 'windows':
                # Windows - use PowerShell
                ps_command = 'Get-Process | Where-Object {$_.MainWindowTitle -ne ""} | Select-Object Id,ProcessName,MainWindowTitle'
                result = subprocess.run(['powershell', '-Command', ps_command],
                                      capture_output=True, text=True, timeout=5)
                if result.returncode == 0:
                    lines = result.stdout.strip().split('\n')[3:]  # Skip header
                    for line in lines:
                        if line.strip():
                            parts = line.strip().split(None, 2)
                            if len(parts) >= 3:
                                windows.append({
                                    'id': parts[0],
                                    'application': parts[1],
                                    'title': parts[2]
                                })
                                
        except Exception as e:
            print(f"Error getting window list: {e}")
            
        return windows
    
    def should_use_terminal(self, action_type: str) -> bool:
        """Determine if terminal should be used for this action type"""
        terminal_preferred = [
            'file_operation',
            'system_info', 
            'text_processing',
            'package_management',
            'network_operation'
        ]
        return action_type in terminal_preferred and self.terminal_capabilities.get(action_type, False)
    
    def should_use_gui(self, action_type: str) -> bool:
        """Determine if GUI should be used for this action type"""
        gui_preferred = [
            'web_browsing',
            'image_editing',
            'video_playback',
            'complex_app_interaction'
        ]
        return action_type in gui_preferred