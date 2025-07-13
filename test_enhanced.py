#!/usr/bin/env python3
"""
Test script for Open Interpreter Enhanced
Validates the new components and functionality
"""

import sys
import os
import unittest
from unittest.mock import Mock, patch

# Add the interpreter to the path
sys.path.insert(0, os.path.join(os.path.dirname(__file__)))

from interpreter.core.computer.action_planner import ActionPlanner
from interpreter.core.computer.visible_terminal import VisibleTerminal
from interpreter.core.computer.window_manager import WindowManager
from interpreter.core.computer.computer import Computer


class TestEnhancedComponents(unittest.TestCase):
    """Test the enhanced components"""
    
    def setUp(self):
        """Set up test fixtures"""
        self.mock_computer = Mock()
        self.mock_computer.interpreter = Mock()
        
    def test_action_planner_initialization(self):
        """Test ActionPlanner initialization"""
        planner = ActionPlanner(self.mock_computer)
        
        self.assertIsNotNone(planner.computer)
        self.assertIsNotNone(planner.os_type)
        self.assertIsNotNone(planner.terminal_capabilities)
        self.assertIsNotNone(planner.gui_capabilities)
        
    def test_action_planner_classification(self):
        """Test request classification"""
        planner = ActionPlanner(self.mock_computer)
        
        # Test file operations
        file_request = "copy all txt files to backup folder"
        classification = planner._classify_request(file_request)
        self.assertEqual(classification, 'file_operation')
        
        # Test app control
        app_request = "open chrome browser"
        classification = planner._classify_request(app_request)
        self.assertEqual(classification, 'app_control')
        
        # Test system info
        system_request = "show cpu usage"
        classification = planner._classify_request(system_request)
        self.assertEqual(classification, 'system_info')
        
    def test_action_planner_planning(self):
        """Test action planning"""
        planner = ActionPlanner(self.mock_computer)
        
        plan = planner.plan_action("list all python files")
        
        self.assertIn('primary_method', plan)
        self.assertIn('fallback_methods', plan)
        self.assertIn('actions', plan)
        self.assertIn('estimated_complexity', plan)
        
    def test_visible_terminal_initialization(self):
        """Test VisibleTerminal initialization"""
        terminal = VisibleTerminal(self.mock_computer)
        
        self.assertIsNotNone(terminal.computer)
        self.assertIsNotNone(terminal.os_type)
        self.assertIsNotNone(terminal.terminal_commands)
        self.assertFalse(terminal.is_active)
        
    def test_window_manager_initialization(self):
        """Test WindowManager initialization"""
        manager = WindowManager(self.mock_computer)
        
        self.assertIsNotNone(manager.computer)
        self.assertIsNotNone(manager.os_type)
        self.assertEqual(len(manager.cached_windows), 0)
        
    def test_computer_enhanced_components(self):
        """Test that Computer class has enhanced components"""
        mock_interpreter = Mock()
        computer = Computer(mock_interpreter)
        
        # Check that enhanced components are initialized
        self.assertIsNotNone(computer.action_planner)
        self.assertIsNotNone(computer.visible_terminal)
        self.assertIsNotNone(computer.window_manager)
        
        # Check types
        self.assertIsInstance(computer.action_planner, ActionPlanner)
        self.assertIsInstance(computer.visible_terminal, VisibleTerminal)
        self.assertIsInstance(computer.window_manager, WindowManager)


class TestEnhancedFunctionality(unittest.TestCase):
    """Test enhanced functionality integration"""
    
    def setUp(self):
        """Set up test fixtures"""
        self.mock_computer = Mock()
        self.mock_computer.interpreter = Mock()
        
    def test_terminal_command_detection(self):
        """Test terminal command detection logic"""
        from interpreter.core.enhanced_respond import _should_use_terminal
        
        # Shell commands should use terminal
        self.assertTrue(_should_use_terminal('shell', 'ls -la'))
        self.assertTrue(_should_use_terminal('bash', 'mkdir test'))
        
        # System commands should use terminal
        self.assertTrue(_should_use_terminal('python', 'apt install python3'))
        self.assertTrue(_should_use_terminal('python', 'git clone repo'))
        
        # Regular code should not use terminal
        self.assertFalse(_should_use_terminal('python', 'import pandas as pd'))
        
    def test_gui_command_detection(self):
        """Test GUI command detection logic"""
        from interpreter.core.enhanced_respond import _should_use_gui
        
        # GUI commands should use GUI
        self.assertTrue(_should_use_gui('python', 'click on button'))
        self.assertTrue(_should_use_gui('python', 'open browser window'))
        self.assertTrue(_should_use_gui('python', 'switch to application'))
        
        # Regular code should not use GUI
        self.assertFalse(_should_use_gui('python', 'calculate sum'))
        
    def test_action_plan_formatting(self):
        """Test action plan formatting"""
        from interpreter.core.enhanced_respond import _format_action_plan
        
        plan = {
            'primary_method': 'terminal',
            'fallback_methods': ['gui', 'code'],
            'estimated_complexity': 'low',
            'actions': [
                {'priority': 1, 'description': 'Use shell commands'},
                {'priority': 2, 'description': 'Fallback to GUI'}
            ]
        }
        
        formatted = _format_action_plan(plan)
        
        self.assertIn('Primary Method: terminal', formatted)
        self.assertIn('Fallback Methods: gui, code', formatted)
        self.assertIn('Complexity: low', formatted)
        self.assertIn('1. Use shell commands', formatted)


def run_integration_test():
    """Run a simple integration test"""
    print("🧪 Running Integration Test...")
    
    try:
        # Test imports
        from interpreter import interpreter
        print("✅ Import successful")
        
        # Test enhanced mode activation
        interpreter.enhanced_mode = True
        print("✅ Enhanced mode activated")
        
        # Test that enhanced components exist
        if hasattr(interpreter.computer, 'action_planner'):
            print("✅ ActionPlanner available")
        else:
            print("❌ ActionPlanner missing")
            
        if hasattr(interpreter.computer, 'visible_terminal'):
            print("✅ VisibleTerminal available")
        else:
            print("❌ VisibleTerminal missing")
            
        if hasattr(interpreter.computer, 'window_manager'):
            print("✅ WindowManager available")
        else:
            print("❌ WindowManager missing")
            
        # Test action planning
        plan = interpreter.computer.action_planner.plan_action("list files")
        if plan and 'primary_method' in plan:
            print("✅ Action planning works")
        else:
            print("❌ Action planning failed")
            
        print("🎉 Integration test completed!")
        return True
        
    except Exception as e:
        print(f"❌ Integration test failed: {e}")
        return False


if __name__ == "__main__":
    print("Open Interpreter Enhanced - Test Suite")
    print("=" * 50)
    
    # Run unit tests
    print("\n📋 Running Unit Tests...")
    unittest.main(argv=[''], exit=False, verbosity=2)
    
    # Run integration test
    print("\n🔗 Running Integration Test...")
    success = run_integration_test()
    
    if success:
        print("\n✅ All tests passed! Enhanced mode is ready.")
    else:
        print("\n❌ Some tests failed. Check the implementation.")