const forestVars = [
  {
    name: "FOREST_KEYSTORE_PHRASE",
    description: "The passphrase for the encrypted keystore",
    value: "any text",
    def: "empty",
    example: "asfvdda",
  },
];

const EnvVar = ({ name, description, value, def, example }) => {
  return (
    <div>
      <h2>{name}</h2>
      <p>{description}</p>
      <p>Value: {value}</p>
      <p>Default: {def}</p>
      <p>Example: {example}</p>
      <code>
        {name}={example}
      </code>
    </div>
  );
};

export const EnvVarTable = () => {
  return (
    <div>
      {forestVars.map((ev) => {
        return <EnvVar {...ev} />;
      })}
    </div>
  );
};
