import './styles.css';

import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';

type UiState = 'idle' | 'recording' | 'transcribing';

let setupScreen: HTMLElement | null = null;
let apiKeyInput: HTMLInputElement | null = null;
let saveApiKeyBtn: HTMLButtonElement | null = null;


let isRecording = false;
let mediaRecorder: MediaRecorder | null = null;
let audioStream: MediaStream | null = null;
let audioChunks: Blob[] = [];
let activePointerId: number | null = null;

function setUiState(state: UiState) {
  const pill = document.getElementById('record-btn') as HTMLButtonElement;
  const status = document.getElementById('status-text') as HTMLElement;
  const subtext = document.getElementById('subtext') as HTMLElement;
  const icon = document.getElementById('record-icon') as HTMLElement;
  const appShell = document.querySelector('.app-shell') as HTMLElement | null;

  pill.classList.remove('recording', 'transcribing');
  appShell?.classList.remove('is-recording', 'is-transcribing');

  if (state === 'recording') {
    pill.classList.add('recording');
    appShell?.classList.add('is-recording');
    status.textContent = 'Listening…';
    subtext.textContent = 'Release to stop';
    icon.textContent = '●';
  } else if (state === 'transcribing') {
    pill.classList.add('transcribing');
    appShell?.classList.add('is-transcribing');
    status.textContent = 'Transcribing…';
    subtext.textContent = 'AI is thinking';
    icon.textContent = '✦';
  } else {
    status.textContent = 'Hold to speak';
    subtext.textContent = 'Option + Space';
    icon.textContent = '◌';
  }
}

async function injectText() {
  const textInput = (document.getElementById('my-text') as HTMLInputElement).value;
  if (!textInput) {
    return;
  }

  await writeText(textInput);
  await invoke('paste_text');

  console.log('DEBUG: Injected text:', textInput);
  setUiState('idle');
}

function showSetupScreen() {
  setupScreen?.classList.remove('hidden');
}

function hideSetupScreen() {
  setupScreen?.classList.add('hidden');
}

async function initializeApiKeyFlow() {
  try {
    await invoke('get_api_key');
    hideSetupScreen();
  } catch (error) {
    showSetupScreen();
  }
}

async function onSaveApiKey() {
  if (!apiKeyInput) {
    return;
  }

  const apiKey = apiKeyInput.value.trim();
  if (!apiKey) {
    alert('Please enter your Groq API key.');
    return;
  }

  try {
    await invoke('save_api_key', { apiKey });
    hideSetupScreen();
  } catch (error) {
    console.error('Save API key error:', error);
    alert(String(error));
  }
}

async function startRecording() {
  const recordBtn = document.getElementById('record-btn') as HTMLButtonElement;
  const inputField = document.getElementById('my-text') as HTMLInputElement;

  if (isRecording) {
    return;
  }

  try {
    audioChunks = [];
    audioStream = await navigator.mediaDevices.getUserMedia({ audio: true });

    mediaRecorder = new MediaRecorder(audioStream);

    mediaRecorder.ondataavailable = (event: BlobEvent) => {
      if (event.data.size > 0) {
        audioChunks.push(event.data);
      }
    };

    mediaRecorder.onstop = async () => {
      isRecording = false;
      const audioBlob = new Blob(audioChunks, { type: 'audio/webm' });
      console.log('Audio recorded successfully. Size:', audioBlob.size, 'bytes');
      console.log('Audio blob created, now processing transcription.');
      setUiState('transcribing');

      try {
        const arrayBuffer = await audioBlob.arrayBuffer();
        const uint8Array = new Uint8Array(arrayBuffer);
        const transcript = await invoke<string>('transcribe_audio', {
          audioData: Array.from(uint8Array),
        });

        inputField.value = transcript;
        await injectText();
        console.log('Transcription complete:', transcript);
      } catch (error) {
        console.error('Transcription error:', error);
        inputField.value = `Error: ${String(error)}`;
        setUiState('idle');
      }

      if (audioStream) {
        audioStream.getTracks().forEach(track => track.stop());
        audioStream = null;
      }
    };

    mediaRecorder.start();
    isRecording = true;
    setUiState('recording');
    recordBtn.setAttribute('aria-pressed', 'true');
  } catch (error) {
    console.error('Mic access error:', error);
    alert('Could not access microphone.');

    if (activePointerId !== null) {
      const button = document.getElementById('record-btn') as HTMLButtonElement | null;
      if (button?.hasPointerCapture(activePointerId)) {
        button.releasePointerCapture(activePointerId);
      }
      activePointerId = null;
    }
  }
}

async function stopRecording() {
  const recordBtn = document.getElementById('record-btn') as HTMLButtonElement;

  if (!isRecording) {
    return;
  }

  if (mediaRecorder && mediaRecorder.state === 'recording') {
    mediaRecorder.stop();
  }

  isRecording = false;
  recordBtn.setAttribute('aria-pressed', 'false');
}

document.addEventListener('DOMContentLoaded', async () => {
  const recordButton = document.getElementById('record-btn') as HTMLButtonElement | null;

  const beginHoldInteraction = (event: PointerEvent) => {
    if (event.pointerType === 'mouse' && event.button !== 0) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();

    if (!recordButton) {
      return;
    }

    if (recordButton.hasPointerCapture(event.pointerId)) {
      recordButton.releasePointerCapture(event.pointerId);
    }

    recordButton.setPointerCapture(event.pointerId);
    activePointerId = event.pointerId;

    void startRecording();
  };

  const endHoldInteraction = (event?: PointerEvent) => {
    if (activePointerId === null) {
      return;
    }

    const pointerId = event?.pointerId ?? activePointerId;
    if (recordButton?.hasPointerCapture(pointerId)) {
      recordButton.releasePointerCapture(pointerId);
    }

    if (isRecording) {
      void stopRecording();
    }

    activePointerId = null;
  };

  recordButton?.addEventListener('pointerdown', beginHoldInteraction);
  recordButton?.addEventListener('pointerup', endHoldInteraction);
  recordButton?.addEventListener('pointercancel', endHoldInteraction);
  recordButton?.addEventListener('lostpointercapture', () => {
    if (activePointerId !== null) {
      void stopRecording();
      activePointerId = null;
    }
  });
  recordButton?.addEventListener('contextmenu', event => event.preventDefault());

  setupScreen = document.getElementById('setup-screen');
  apiKeyInput = document.getElementById('api-key-input') as HTMLInputElement | null;
  saveApiKeyBtn = document.getElementById('save-api-key-btn') as HTMLButtonElement | null;

  saveApiKeyBtn?.addEventListener('click', onSaveApiKey);

  await initializeApiKeyFlow();

  window.addEventListener('keydown', event => {
    const key = event.key.toLowerCase();
    if (event.target instanceof HTMLInputElement) {
      return;
    }

    if (key === 'd' || key === 'escape') {
      event.preventDefault();
      if (!event.repeat) {
        if (isRecording) {
          void stopRecording();
        } else {
          void startRecording();
        }
      }
    }
  });

  // Set up window baseline placement properties
  const appWindow = getCurrentWindow();
  await appWindow.setAlwaysOnTop(true);
  setUiState('idle');

  // Correctly formatted Tauri background IPC event routing channel
  await listen('toggle-recording', () => {
    if (isRecording) {
      void stopRecording();
    } else {
      void startRecording();
    }
  });
});
