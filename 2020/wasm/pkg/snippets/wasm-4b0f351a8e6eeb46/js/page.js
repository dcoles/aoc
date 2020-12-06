export function log(message) {
    const console = document.getElementById("console");
    console.textContent += message + '\n';
    console.scrollTop = console.scrollHeight;
}
