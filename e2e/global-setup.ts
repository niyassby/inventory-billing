/** Creates the item the tests sell, through the same API the screens use. */
const BRIDGE = "http://127.0.0.1:1431";
export const EMAIL = "demo@example.test";
export const PASSWORD = "Demo-shop-2026";

async function call<T>(command: string, token: string, input: unknown): Promise<T> {
  const response = await fetch(`${BRIDGE}/v1/call/${command}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ token, input }),
  });
  if (!response.ok) throw new Error(`${command} failed: ${await response.text()}`);
  return (await response.json()) as T;
}

export default async function globalSetup(): Promise<void> {
  const session = await call<{ token: string }>("auth_login", "", {
    email: EMAIL,
    password: PASSWORD,
  });
  const units = await call<{ id: string; code: string }[]>("units_list", session.token, {});
  const unit = units[0];
  if (!unit) throw new Error("The demo shop has no units");
  await call("products_save", session.token, {
    name: "E2E Basmati Rice",
    baseUnitId: unit.id,
    variants: [
      {
        sku: "E2E-RICE",
        barcode: "8900000000017",
        salePrice: 12_500,
        costPrice: 9_000,
        openingStock: 50_000,
      },
    ],
  });
}
