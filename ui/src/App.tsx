import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

function App() {
  const [response, setResponse] = useState<string>('');

  async function handlePing() {
    try {
      const result = await invoke<string>('ping');
      setResponse(result);
    } catch (error) {
      setResponse(`Error: ${error}`);
    }
  }

  return (
    <div className="min-h-screen bg-gray-100 flex items-center justify-center">
      <div className="bg-white p-8 rounded-lg shadow-md">
        <h1 className="text-2xl font-bold mb-4">FileSync</h1>
        <button
          onClick={handlePing}
          className="bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded"
        >
          Ping Backend
        </button>
        {response && (
          <p className="mt-4 text-green-600">Response: {response}</p>
        )}
      </div>
    </div>
  );
}

export default App;
