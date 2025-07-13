#!/usr/bin/env python3
"""
Demo script for Open Interpreter Enhanced
Shows the new capabilities and prioritization system
"""

import sys
import os

# Add the interpreter to the path
sys.path.insert(0, os.path.join(os.path.dirname(__file__)))

from interpreter import interpreter

def demo_enhanced_features():
    """Demonstrate the enhanced features"""
    
    print("🚀 Open Interpreter Enhanced Demo")
    print("=" * 50)
    
    # Enable enhanced mode
    interpreter.enhanced_mode = True
    
    print("\n✅ Enhanced mode activated!")
    print("Features enabled:")
    print("  - Intelligent action planning")
    print("  - Visible terminal execution")
    print("  - Advanced window management")
    print("  - Priority-based task execution")
    
    # Test cases to demonstrate different capabilities
    test_cases = [
        {
            "name": "File Operations (Terminal Priority)",
            "command": "List all Python files in the current directory and show their sizes",
            "expected_method": "terminal"
        },
        {
            "name": "System Information (Terminal Priority)", 
            "command": "Show me the current system information including CPU, memory, and disk usage",
            "expected_method": "terminal"
        },
        {
            "name": "Window Management (GUI Priority)",
            "command": "Show me what applications are currently open",
            "expected_method": "gui"
        },
        {
            "name": "Complex Task (Code as Last Resort)",
            "command": "Create a Python script that analyzes the frequency of words in a text file",
            "expected_method": "code"
        }
    ]
    
    print(f"\n📋 Running {len(test_cases)} test cases...")
    
    for i, test_case in enumerate(test_cases, 1):
        print(f"\n{'='*60}")
        print(f"Test {i}: {test_case['name']}")
        print(f"Expected Method: {test_case['expected_method']}")
        print(f"Command: {test_case['command']}")
        print(f"{'='*60}")
        
        try:
            # Send the command to the interpreter
            response = interpreter.chat(test_case['command'])
            print(f"✅ Test {i} completed successfully")
            
        except Exception as e:
            print(f"❌ Test {i} failed: {e}")
        
        print(f"\n{'='*60}")
        
        # Ask user if they want to continue
        if i < len(test_cases):
            user_input = input(f"\nPress Enter to continue to test {i+1}, or 'q' to quit: ")
            if user_input.lower() == 'q':
                break
    
    print("\n🎉 Demo completed!")
    print("\nEnhanced features demonstrated:")
    print("  ✓ Action planning and prioritization")
    print("  ✓ Visible terminal execution")
    print("  ✓ Window management capabilities")
    print("  ✓ Intelligent method selection")


def interactive_mode():
    """Run in interactive mode to test enhanced features"""
    
    print("\n🔧 Interactive Enhanced Mode")
    print("=" * 40)
    print("Enhanced mode is active. Try these commands:")
    print("  - File operations: 'create a backup of all .py files'")
    print("  - System tasks: 'show me running processes'")
    print("  - Window management: 'what applications are open?'")
    print("  - Type 'quit' to exit")
    print()
    
    interpreter.enhanced_mode = True
    
    while True:
        try:
            user_input = input("Enhanced> ").strip()
            
            if user_input.lower() in ['quit', 'exit', 'q']:
                break
                
            if not user_input:
                continue
                
            # Process the command
            response = interpreter.chat(user_input)
            
        except KeyboardInterrupt:
            print("\n\nExiting...")
            break
        except Exception as e:
            print(f"Error: {e}")


if __name__ == "__main__":
    print("Open Interpreter Enhanced Demo")
    print("Choose an option:")
    print("1. Run automated demo")
    print("2. Interactive mode")
    print("3. Exit")
    
    choice = input("\nEnter your choice (1-3): ").strip()
    
    if choice == "1":
        demo_enhanced_features()
    elif choice == "2":
        interactive_mode()
    elif choice == "3":
        print("Goodbye!")
    else:
        print("Invalid choice. Exiting.")