"""
Enhanced Response System for Open Interpreter
Integrates action planning, visible terminal, and window management
"""

import json
import os
import re
import time
import traceback
from typing import Dict, List, Optional, Generator

from .computer.action_planner import ActionPlanner
from .computer.visible_terminal import VisibleTerminal
from .computer.window_manager import WindowManager
from .render_message import render_message


def enhanced_respond(interpreter) -> Generator[Dict, None, None]:
    """
    Enhanced response system that prioritizes terminal commands,
    then GUI interactions, then code execution as last resort
    """
    
    # Initialize enhanced components
    if not hasattr(interpreter.computer, 'action_planner'):
        interpreter.computer.action_planner = ActionPlanner(interpreter.computer)
    
    if not hasattr(interpreter.computer, 'visible_terminal'):
        interpreter.computer.visible_terminal = VisibleTerminal(interpreter.computer)
    
    if not hasattr(interpreter.computer, 'window_manager'):
        interpreter.computer.window_manager = WindowManager(interpreter.computer)
    
    last_unsupported_code = ""
    insert_loop_message = False

    while True:
        ## ENHANCED SYSTEM MESSAGE RENDERING ##
        
        system_message = interpreter.system_message
        
        # Add enhanced capabilities to system message
        enhanced_instructions = """

ENHANCED CAPABILITIES:
You now have advanced computer control capabilities. Use them in this priority order:

1. TERMINAL COMMANDS (HIGHEST PRIORITY)
   - Use shell commands for file operations, system tasks, installations
   - Commands will be shown in a visible terminal window
   - Examples: ls, cp, mv, mkdir, wget, curl, apt install, brew install

2. GUI INTERACTIONS (MEDIUM PRIORITY)  
   - Use mouse and keyboard for applications that require GUI
   - Can detect and switch between open windows/applications
   - Examples: web browsing, image editing, complex app interactions

3. CODE EXECUTION (LOWEST PRIORITY)
   - Only use when terminal commands and GUI can't accomplish the task
   - For complex programming, data analysis, or custom logic

WINDOW MANAGEMENT:
- You can see all open windows and applications
- You can switch between them intelligently
- You can open new applications as needed

VISIBLE TERMINAL:
- All terminal commands are shown in a visible window
- Users can see exactly what you're executing
- This builds trust and transparency

Always explain your approach and why you chose a particular method.
"""
        
        system_message += enhanced_instructions
        
        # Add language-specific system messages
        for language in interpreter.computer.terminal.languages:
            if hasattr(language, "system_message"):
                system_message += "\n\n" + language.system_message

        # Add custom instructions
        if interpreter.custom_instructions:
            system_message += "\n\n" + interpreter.custom_instructions

        # Add computer API system message
        if interpreter.computer.import_computer_api:
            if interpreter.computer.system_message not in system_message:
                system_message = (
                    system_message + "\n\n" + interpreter.computer.system_message
                )

        ## Rendering ↓
        rendered_system_message = render_message(interpreter, system_message)
        ## Rendering ↑

        rendered_system_message = {
            "role": "system",
            "type": "message",
            "content": rendered_system_message,
        }

        # Create the version of messages that we'll send to the LLM
        messages_for_llm = interpreter.messages.copy()
        messages_for_llm = [rendered_system_message] + messages_for_llm

        if insert_loop_message:
            messages_for_llm.append(
                {
                    "role": "user",
                    "type": "message",
                    "content": interpreter.loop_message,
                }
            )
            yield {"role": "assistant", "type": "message", "content": "\n\n"}
            insert_loop_message = False

        ### ENHANCED DECISION MAKING ###
        
        # Get the last user message to analyze
        user_messages = [m for m in interpreter.messages if m["role"] == "user"]
        if user_messages:
            last_user_message = user_messages[-1]["content"]
            
            # Create action plan
            action_plan = interpreter.computer.action_planner.plan_action(last_user_message)
            
            # Add context about current state
            context_info = _get_system_context(interpreter)
            if context_info:
                yield {
                    "role": "assistant", 
                    "type": "message", 
                    "content": f"**System Context:**\n{context_info}\n\n"
                }
            
            # Add action plan to the conversation
            plan_description = _format_action_plan(action_plan)
            yield {
                "role": "assistant", 
                "type": "message", 
                "content": f"**Action Plan:**\n{plan_description}\n\n"
            }

        ### RUN THE LLM ###

        assert (
            len(interpreter.messages) > 0
        ), "User message was not passed in. You need to pass in at least one message."

        if interpreter.messages[-1]["type"] != "code":
            try:
                for chunk in interpreter.llm.run(messages_for_llm):
                    yield {"role": "assistant", **chunk}

            except Exception as e:
                # Handle LLM errors (same as original)
                error_message = str(e).lower()
                if (
                    interpreter.offline == False
                    and "auth" in error_message
                    or "api key" in error_message
                ):
                    output = traceback.format_exc()
                    raise Exception(
                        f"{output}\n\nThere might be an issue with your API key(s)."
                    )
                else:
                    raise

        ### ENHANCED CODE EXECUTION ###

        if interpreter.messages[-1]["type"] == "code":
            if interpreter.verbose:
                print("Enhanced execution:", interpreter.messages[-1])

            try:
                language = interpreter.messages[-1]["format"].lower().strip()
                code = interpreter.messages[-1]["content"]

                # Clean up code
                if code.startswith("`\n"):
                    code = code[2:].strip()
                    interpreter.messages[-1]["content"] = code

                # Detect if this should be a terminal command
                if _should_use_terminal(language, code):
                    yield from _execute_terminal_command(interpreter, code)
                    continue

                # Detect if this should be a GUI interaction
                elif _should_use_gui(language, code):
                    yield from _execute_gui_interaction(interpreter, code)
                    continue

                # Otherwise, execute as regular code
                else:
                    yield from _execute_regular_code(interpreter, language, code)
                    continue

            except GeneratorExit:
                break
            except Exception as e:
                content = traceback.format_exc()
                yield {"role": "computer", "type": "console", "format": "output", "content": content}

        # Check if we should continue the loop
        if interpreter.loop:
            # Check for loop breakers
            if interpreter.messages and interpreter.messages[-1]["role"] == "assistant":
                last_content = interpreter.messages[-1]["content"]
                if any(breaker in last_content for breaker in interpreter.loop_breakers):
                    break
            insert_loop_message = True
        else:
            break


def _get_system_context(interpreter) -> str:
    """Get current system context (windows, processes, etc.)"""
    context = []
    
    try:
        # Get window information
        if hasattr(interpreter.computer, 'window_manager'):
            window_summary = interpreter.computer.window_manager.get_window_summary()
            context.append(f"Open Windows:\n{window_summary}")
        
        # Get current directory
        current_dir = os.getcwd()
        context.append(f"Current Directory: {current_dir}")
        
        # Get recent command history if available
        if (hasattr(interpreter.computer, 'visible_terminal') and 
            interpreter.computer.visible_terminal.command_history):
            recent_commands = interpreter.computer.visible_terminal.command_history[-3:]
            if recent_commands:
                context.append("Recent Commands:")
                for cmd in recent_commands:
                    context.append(f"  - {cmd['command']}")
    
    except Exception as e:
        context.append(f"Error getting context: {e}")
    
    return "\n".join(context) if context else ""


def _format_action_plan(plan: Dict) -> str:
    """Format the action plan for display"""
    lines = []
    
    lines.append(f"Primary Method: {plan['primary_method']}")
    
    if plan['fallback_methods']:
        lines.append(f"Fallback Methods: {', '.join(plan['fallback_methods'])}")
    
    lines.append(f"Complexity: {plan['estimated_complexity']}")
    
    if plan['actions']:
        lines.append("\nPlanned Actions:")
        for action in plan['actions']:
            lines.append(f"  {action['priority']}. {action['description']}")
    
    return "\n".join(lines)


def _should_use_terminal(language: str, code: str) -> bool:
    """Determine if code should be executed as terminal command"""
    
    # Shell/bash commands should always use terminal
    if language in ['shell', 'bash', 'sh', 'zsh', 'fish']:
        return True
    
    # Simple system commands in other languages
    terminal_patterns = [
        r'^(ls|dir|pwd|cd|mkdir|rmdir|rm|cp|mv|cat|grep|find|which|whereis)',
        r'^(wget|curl|ping|ssh|scp|rsync)',
        r'^(apt|yum|brew|pip|npm|yarn)\s+install',
        r'^(systemctl|service|ps|top|htop|kill)',
        r'^(git|svn|hg)\s+',
    ]
    
    code_lines = code.strip().split('\n')
    first_line = code_lines[0].strip()
    
    for pattern in terminal_patterns:
        if re.match(pattern, first_line, re.IGNORECASE):
            return True
    
    return False


def _should_use_gui(language: str, code: str) -> bool:
    """Determine if code should be executed as GUI interaction"""
    
    gui_patterns = [
        r'(click|mouse|keyboard|screen|window)',
        r'(browser|navigate|url|website)',
        r'(application|app|program).*open',
        r'(switch|focus|activate).*window',
    ]
    
    for pattern in gui_patterns:
        if re.search(pattern, code, re.IGNORECASE):
            return True
    
    return False


def _execute_terminal_command(interpreter, code: str) -> Generator[Dict, None, None]:
    """Execute code as terminal command in visible terminal"""
    
    yield {
        "role": "computer",
        "type": "message", 
        "content": "Executing in visible terminal..."
    }
    
    try:
        # Use visible terminal
        result = interpreter.computer.visible_terminal.execute_visible_command(code)
        
        if isinstance(result, dict):
            yield {"role": "computer", **result}
        else:
            # Handle streaming results
            for chunk in result:
                yield {"role": "computer", **chunk}
                
    except Exception as e:
        yield {
            "role": "computer",
            "type": "console",
            "format": "output",
            "content": f"Terminal execution error: {e}"
        }


def _execute_gui_interaction(interpreter, code: str) -> Generator[Dict, None, None]:
    """Execute code as GUI interaction"""
    
    yield {
        "role": "computer",
        "type": "message",
        "content": "Executing GUI interaction..."
    }
    
    try:
        # Parse GUI commands and execute them
        if "click" in code.lower():
            # Handle mouse clicks
            yield from _handle_mouse_action(interpreter, code)
        elif "type" in code.lower() or "keyboard" in code.lower():
            # Handle keyboard input
            yield from _handle_keyboard_action(interpreter, code)
        elif "window" in code.lower():
            # Handle window management
            yield from _handle_window_action(interpreter, code)
        else:
            # Fallback to regular code execution
            yield from _execute_regular_code(interpreter, "python", code)
            
    except Exception as e:
        yield {
            "role": "computer",
            "type": "console",
            "format": "output",
            "content": f"GUI interaction error: {e}"
        }


def _handle_mouse_action(interpreter, code: str) -> Generator[Dict, None, None]:
    """Handle mouse-related actions"""
    
    # Take screenshot first
    screenshot = interpreter.computer.display.screenshot()
    if screenshot:
        yield {
            "role": "computer",
            "type": "image",
            "format": "base64.png",
            "content": screenshot
        }
    
    # Execute mouse action using computer.mouse
    try:
        # This would need to parse the code and extract coordinates/actions
        # For now, execute as Python code
        yield from _execute_regular_code(interpreter, "python", code)
    except Exception as e:
        yield {
            "role": "computer",
            "type": "console", 
            "format": "output",
            "content": f"Mouse action error: {e}"
        }


def _handle_keyboard_action(interpreter, code: str) -> Generator[Dict, None, None]:
    """Handle keyboard-related actions"""
    
    try:
        # Execute keyboard action using computer.keyboard
        yield from _execute_regular_code(interpreter, "python", code)
    except Exception as e:
        yield {
            "role": "computer",
            "type": "console",
            "format": "output", 
            "content": f"Keyboard action error: {e}"
        }


def _handle_window_action(interpreter, code: str) -> Generator[Dict, None, None]:
    """Handle window management actions"""
    
    try:
        # Get current windows
        windows = interpreter.computer.window_manager.get_all_windows()
        
        yield {
            "role": "computer",
            "type": "message",
            "content": f"Found {len(windows)} open windows. Processing window action..."
        }
        
        # Execute window action
        yield from _execute_regular_code(interpreter, "python", code)
        
    except Exception as e:
        yield {
            "role": "computer",
            "type": "console",
            "format": "output",
            "content": f"Window action error: {e}"
        }


def _execute_regular_code(interpreter, language: str, code: str) -> Generator[Dict, None, None]:
    """Execute code using the regular Open Interpreter method"""
    
    # Validate language
    if interpreter.computer.terminal.get_language(language) == None:
        yield {
            "role": "computer",
            "type": "console",
            "format": "output",
            "content": f"`{language}` disabled or not supported.",
        }
        return

    # Check for empty code
    if code.strip() == "":
        yield {
            "role": "computer",
            "type": "console",
            "format": "output",
            "content": "Code block was empty. Please try again.",
        }
        return

    # Yield confirmation
    try:
        yield {
            "role": "computer",
            "type": "confirmation",
            "format": "execution",
            "content": {
                "type": "code",
                "format": language,
                "content": code,
            },
        }
    except GeneratorExit:
        return

    # Execute the code
    try:
        for line in interpreter.computer.run(language, code, stream=True):
            yield {"role": "computer", **line}
    except Exception as e:
        yield {
            "role": "computer",
            "type": "console",
            "format": "output",
            "content": f"Execution error: {e}"
        }