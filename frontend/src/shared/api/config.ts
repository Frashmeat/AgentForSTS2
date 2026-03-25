export async function loadAppConfig() {
  const response = await fetch("/api/config");
  return response.json();
}
