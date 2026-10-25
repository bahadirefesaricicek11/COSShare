import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import QRCode from "qrcode";

let tunnelUrl = "";
let isLanMode = false;
let currentFilePath = "";
let currentFileName = "";

const dropZoneFile = document.getElementById("dropZoneFile")!;
const statusCard = document.getElementById("statusCard")!;
const statusBadge = document.getElementById("statusBadge")!;
const fileNameDisplay = document.getElementById("fileNameDisplay")!;
const urlBox = document.getElementById("urlBox")!;
const qrContainer = document.getElementById("qrContainer")!;
const qrImage = document.getElementById("qrImage") as HTMLImageElement;
const stopBtn = document.getElementById("stopBtn")!;
const toast = document.getElementById("toast")!;

const modeGlobalBtn = document.getElementById("modeGlobalBtn")!;
const modeLanBtn = document.getElementById("modeLanBtn")!;

// Listen for remote download events from Rust
listen<string>("download-started", (event) => {
  statusBadge.innerText = "Downloading...";
  statusBadge.className = "status-badge active pulse";
  
  toast.innerText = `Downloading started: ${event.payload}`;
  toast.classList.add("show");
  setTimeout(() => toast.classList.remove("show"), 4000);
});

// Mode Switching
modeGlobalBtn.addEventListener("click", () => {
  if (!isLanMode) return;
  isLanMode = false;
  modeGlobalBtn.classList.add("active");
  modeLanBtn.classList.remove("active");
  if (currentFilePath) handleShare(currentFilePath, currentFileName);
});

modeLanBtn.addEventListener("click", () => {
  if (isLanMode) return;
  isLanMode = true;
  modeLanBtn.classList.add("active");
  modeGlobalBtn.classList.remove("active");
  if (currentFilePath) handleShare(currentFilePath, currentFileName);
});

dropZoneFile.addEventListener("click", async () => {
  try {
    const selected = await open({
      multiple: false,
      directory: false,
    });

    if (selected) {
      currentFilePath = selected as string;
      currentFileName = currentFilePath.split(/[/\\]/).pop() || "file";
      await handleShare(currentFilePath, currentFileName);
    }
  } catch (err) {
    console.error("Dialog error:", err);
  }
});

async function handleShare(filePath: string, fileName: string) {
  statusCard.style.display = "block";
  statusBadge.innerText = "Sharing";
  statusBadge.className = "status-badge";
  fileNameDisplay.innerText = fileName;
  urlBox.innerText = isLanMode ? "Resolving LAN address..." : "Starting HTTPS tunnel...";

  try {
    const code = await invoke<string>("register_file", { filePath });
    let baseUrl = "";

    if (isLanMode) {
      const localIp = await invoke<string>("get_local_ip");
      baseUrl = `http://${localIp}:8080`;
    } else {
      if (!tunnelUrl) {
        tunnelUrl = await invoke<string>("start_tunnel", { port: 8080 });
      }
      baseUrl = tunnelUrl;
    }

    // Inside handleShare function, right after setting shareLink:
    const shareLink = `${baseUrl}/s/${code}`;
    urlBox.innerText = shareLink;
    statusBadge.innerText = "Active";
    statusBadge.className = "status-badge active";

    // Automatically copy to clipboard instantly
    navigator.clipboard.writeText(shareLink);
    toast.innerText = "Link automatically copied to clipboard!";
    toast.classList.add("show");
    setTimeout(() => toast.classList.remove("show"), 2500);

    const qrDataUrl = await QRCode.toDataURL(shareLink, { width: 116, margin: 1 });
    qrImage.src = qrDataUrl;
    qrContainer.style.display = "flex";
  } catch (err: any) {
    urlBox.innerText = typeof err === "string" ? err : (err?.message || "Error starting share");
    statusBadge.innerText = "Error";
    console.error(err);
  }
}

urlBox.addEventListener("click", () => {
  if (urlBox.innerText.startsWith("http")) {
    navigator.clipboard.writeText(urlBox.innerText);
    toast.innerText = "Link copied to clipboard!";
    toast.classList.add("show");
    setTimeout(() => toast.classList.remove("show"), 2000);
  }
});

stopBtn.addEventListener("click", () => {
  statusCard.style.display = "none";
  qrContainer.style.display = "none";
  currentFilePath = "";
  currentFileName = "";
  urlBox.innerText = "Generating tunnel link...";
});