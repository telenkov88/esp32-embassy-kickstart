let input = document.getElementsByTagName("input")[0];
let output = document.getElementById("output");
let button = document.getElementsByTagName("button")[0];

let ssid =  document.getElementById('ssidInput')
let psw =  document.getElementById('password')
let hostname =  document.getElementById('hostName')
let uploadButton = document.getElementById("uploadDataBtn")

// Regex for a “valid” SSID - 1–32 printable characters:
// We allow letters, numbers, spaces, dashes, underscores, and dots.
const ssidPattern = /^[\w .\-]{1,32}$/;

// WPA2-PSK regex: exactly 8–63 printable ASCII chars.
const pswPattern = /^[\x20-\x7E]{8,63}$/;

// Hostname regex (1–15 alphanumeric or dash).
const hostnamePattern = /^[a-zA-Z0-9\-]{1,15}$/;


ssid.addEventListener("input", validateInputs);
psw.addEventListener("input", validateInputs);
hostname.addEventListener("input", validateInputs);
validateInputs();
function checkPattern(inputElement, pattern) {
    const value = inputElement.value.trim();
    return pattern.test(value);
}

function validateInputs() {
    const ssidValid = checkPattern(ssid, ssidPattern);
    const pswValid = checkPattern(psw, pswPattern);
    const hostnameValid = checkPattern(hostname, hostnamePattern);

    const valid = (ssidValid && pswValid && hostnameValid);
    uploadButton.disabled = !valid
    console.log("button status disabled=", uploadButton.disabled)
    return valid
}

uploadButton.addEventListener("click", function () {
    const data = {
        "ssid": document.getElementById('ssidInput').value,
        "psw": document.getElementById('password').value,
        "hostname": document.getElementById('hostName').value,
    };
    if (!validateInputs()){
        console.log("Settings invalid", data)
        return
    }
    console.log("upload data", data)

    const url = window.location.href + "settings";
    fetch(url, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify(data),
    })
        .then(response => {
            console.log("Response:", response)
        })
        .then(data => {
            console.log('Upload success:', data);
        })
        .catch((error) => {
            console.error('Error:', error);
        });
});


input.addEventListener("input", function () {
    button.disabled = !input.value;
});

const currentPath = window.location.pathname;

let websocketUri = (window.location.protocol === "https:") ? "wss:" : "ws:";
websocketUri += "//" + window.location.host;
websocketUri += currentPath.slice(0, currentPath.lastIndexOf("/") + 1) + "ws";

let ws = new WebSocket(websocketUri, ["echo", "ignored_protocol"]);

ws.addEventListener("close", function () {
    ws.close();
    output.innerText = "Events Closed";
})

ws.addEventListener("error", function (ev) {
    ws.close();
    console.error(ev);
    output.innerText = "Events Error";
});

ws.addEventListener("message", function (ev) {
    let message = document.createElement("li");
    message.innerText = ev.data;
    output.appendChild(message);
});

button.addEventListener("click", function () {
    ws.send(input.value);

    input.value = "";
});

let events = new EventSource("events");

events.addEventListener("error", function () {
    events.close();
    console.log("Events Closed");
});

events.addEventListener("message_changed", function (ev) {
    console.log("Got SSE data", ev.data);
})


